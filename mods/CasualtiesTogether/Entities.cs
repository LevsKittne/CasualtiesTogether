using System;
using System.Collections.Generic;
using System.IO;
using System.Reflection;
using System.Reflection.Emit;
using HarmonyLib;
using UnityEngine;

namespace CasualtiesTogether {
    sealed class NetEntity : MonoBehaviour {
        public uint Id;
        public uint Epoch;
        public bool RemoteDestroyed;
        public byte EnemyKind;
        public Rigidbody2D Rb;
        public BuildingEntity Bld;
        public Animator Anim;
        public bool BldKnown;

        public BuildingEntity Building() {
            if (!BldKnown) {
                BldKnown = true;
                Bld = GetComponent<BuildingEntity>();
            }
            return Bld;
        }

        void OnDestroy() {
            Entities.OnDestroyed(this);
        }
    }

    static class Entities {
        const ushort MethodDestroy = 0xFFFF;
        const byte FlagPersist = 1;

        enum Mode {
            Trigger,
            HostAction
        }

        sealed class Rpc {
            public ushort Index;
            public MethodInfo Method;
            public Mode Mode;
            public bool Persist;
            public ParameterInfo[] Params;
        }

        struct CallState {
            public bool Seeded;
            public UnityEngine.Random.State Outer;
        }

        static readonly Type[] Tracked = {
            typeof(BearTrap), typeof(MineScript), typeof(GunmineScript), typeof(FanTrap), typeof(SawbladeScript),
            typeof(SpikeStabberScript), typeof(TurretScript), typeof(CeilingTurret), typeof(CosmeticDoor), typeof(TraderScript),
            typeof(StalactiteDropper), typeof(DamagingCrate), typeof(LampScript), typeof(JumpPadScript), typeof(PistonScript),
            typeof(GeyserScript), typeof(BarbedFence), typeof(BuildingEntity), typeof(MedStationScript), typeof(TerminalScript),
            typeof(RecipeBoxScript), typeof(GrabberPlant), typeof(PusSackScript), typeof(BounceShroom), typeof(CactusScript),
            typeof(LeadbushScript), typeof(GuardSpawner), typeof(CaveTickSpawner), typeof(SoundCannon), typeof(Container)
        };

        static readonly List<Rpc> rpcs = new List<Rpc>();
        static readonly Dictionary<MethodBase, Rpc> rpcByMethod = new Dictionary<MethodBase, Rpc>();
        static readonly Dictionary<uint, NetEntity> byId = new Dictionary<uint, NetEntity>();
        static readonly byte[] pollBuf = new byte[65536];
        static readonly Dictionary<int, Vector2> spawnPos = new Dictionary<int, Vector2>();
        static readonly Dictionary<ulong, float> lastTrigger = new Dictionary<ulong, float>();
        const float TriggerThrottle = 0.3f;
        static readonly System.Random seeds = new System.Random();

        static ModLoader.Logger log;
        static Func<bool> session;
        static Func<bool> isHost;
        static uint epoch;
        static bool applying;
        static bool quitting;

        public static uint Salt;
        public static int Count => byId.Count;

        public static void Install(Harmony harmony, ModLoader.Logger logger, Func<bool> inSession, Func<bool> host) {
            log = logger;
            session = inSession;
            isHost = host;
            Add(typeof(BearTrap), "OnTriggerEnter2D", Mode.Trigger, false);
            Add(typeof(SpikeStabberScript), "OnTriggerEnter2D", Mode.Trigger, false);
            Add(typeof(GeyserScript), "OnTriggerEnter2D", Mode.Trigger, false);
            Add(typeof(GeyserScript), "Activate", Mode.HostAction, false);
            Add(typeof(StalactiteDropper), "Drop", Mode.HostAction, true);
            Add(typeof(FanTrap), "BreakFan", Mode.HostAction, true);
            Add(typeof(Container), "ContainerBroke", Mode.HostAction, true);
            var prefix = new HarmonyMethod(typeof(Entities), nameof(RpcPrefix)) { priority = Priority.First };
            var postfix = new HarmonyMethod(typeof(Entities), nameof(RpcPostfix));
            var transpiler = new HarmonyMethod(typeof(Entities), nameof(ContextTranspiler));
            foreach (var r in rpcs) {
                harmony.Patch(r.Method, prefix: prefix, postfix: postfix, transpiler: transpiler);
            }
            var startPrefix = new HarmonyMethod(typeof(Entities), nameof(StartPrefix));
            var startPostfix = new HarmonyMethod(typeof(Entities), nameof(StartPostfix));
            var starts = 0;
            foreach (var t in Tracked) {
                var start = AccessTools.DeclaredMethod(t, "Start");
                if (start != null && start.GetMethodBody() != null) {
                    harmony.Patch(start, prefix: startPrefix, postfix: startPostfix);
                    starts++;
                }
            }
            var awakePostfix = new HarmonyMethod(typeof(Entities), nameof(AwakePostfix));
            foreach (var t in Tracked) {
                var awake = AccessTools.DeclaredMethod(t, "Awake");
                if (awake != null && awake.GetMethodBody() != null) {
                    harmony.Patch(awake, postfix: awakePostfix);
                }
            }
            log.Info($"entity sync: {Tracked.Length} tracked types, {rpcs.Count} replicated methods, {starts} seeded Start methods");
        }

        static void Add(Type t, string name, Mode mode, bool persist) {
            var m = AccessTools.DeclaredMethod(t, name);
            if (m == null) {
                log.Warning($"entity sync: {t.Name}.{name} not found");
                return;
            }
            var r = new Rpc { Index = (ushort)rpcs.Count, Method = m, Mode = mode, Persist = persist, Params = m.GetParameters() };
            rpcs.Add(r);
            rpcByMethod[m] = r;
        }

        static void AwakePostfix(Component __instance) {
            spawnPos[__instance.gameObject.GetInstanceID()] = __instance.transform.position;
        }

        public static void OnQuit() {
            quitting = true;
        }

        public static void Register() {
            epoch++;
            byId.Clear();
            lastTrigger.Clear();
            var list = new List<(int type, Vector2 pos, string name, GameObject go)>();
            var seen = new HashSet<GameObject>();
            for (var ti = 0; ti < Tracked.Length; ti++) {
                foreach (var o in UnityEngine.Object.FindObjectsOfType(Tracked[ti])) {
                    var go = ((Component)o).gameObject;
                    if (!seen.Add(go) || go.GetComponentInParent<Item>() != null || go.GetComponentInParent<Body>() != null) {
                        continue;
                    }
                    var pos = spawnPos.TryGetValue(go.GetInstanceID(), out var sp) ? sp : (Vector2)go.transform.position;
                    list.Add((ti, pos, go.name, go));
                }
            }
            list.Sort((a, b) => {
                var c = a.type.CompareTo(b.type);
                if (c != 0) return c;
                c = Mathf.RoundToInt(a.pos.x * 100f).CompareTo(Mathf.RoundToInt(b.pos.x * 100f));
                if (c != 0) return c;
                c = Mathf.RoundToInt(a.pos.y * 100f).CompareTo(Mathf.RoundToInt(b.pos.y * 100f));
                return c != 0 ? c : string.CompareOrdinal(a.name, b.name);
            });
            spawnPos.Clear();
            uint id = 0;
            foreach (var e in list) {
                var ne = e.go.GetComponent<NetEntity>() ?? e.go.AddComponent<NetEntity>();
                ne.Id = ++id;
                ne.Epoch = epoch;
                ne.RemoteDestroyed = false;
                byId[ne.Id] = ne;
            }
            log.Info($"registered {byId.Count} world entities, layout {LayoutHash():X8}");
            if (Together.Verbose) {
                var per = new SortedDictionary<string, (int n, uint h, int moving)>();
                foreach (var e in list) {
                    var key = Tracked[e.type].Name;
                    per.TryGetValue(key, out var v);
                    var rb = e.go.GetComponent<Rigidbody2D>();
                    var moving = rb != null && rb.bodyType == RigidbodyType2D.Dynamic ? 1 : 0;
                    v.n++;
                    v.h = (v.h ^ (uint)Mathf.RoundToInt(e.pos.x * 100f) ^ ((uint)Mathf.RoundToInt(e.pos.y * 100f) << 7)) * 16777619u;
                    v.moving += moving;
                    per[key] = v;
                }
                var parts = new List<string>();
                foreach (var kv in per) {
                    parts.Add($"{kv.Key}:{kv.Value.n}/{kv.Value.moving}/{kv.Value.h:X8}");
                }
                log.Debug("entity types: " + string.Join(" ", parts));
            }
        }

        public static uint LayoutHash() {
            var h = 2166136261u;
            foreach (var kv in byId) {
                if (kv.Value == null) {
                    continue;
                }
                h = (h ^ kv.Key) * 16777619u;
                h = (h ^ Fnv(kv.Value.name)) * 16777619u;
            }
            return h;
        }

        public static string Describe(uint id) {
            return byId.TryGetValue(id, out var ne) && ne != null ? $"#{id} {ne.name} at {(Vector2)ne.transform.position}" : $"#{id} not found";
        }

        public static NetEntity ById(uint id) {
            return byId.TryGetValue(id, out var ne) && ne != null ? ne : null;
        }

        public static Dictionary<uint, NetEntity>.ValueCollection Values => byId.Values;

        public static IEnumerable<NetEntity> All() {
            foreach (var ne in byId.Values) {
                if (ne != null) {
                    yield return ne;
                }
            }
        }

        public static NetEntity AddRuntime(GameObject go, uint id) {
            var ne = go.GetComponent<NetEntity>() ?? go.AddComponent<NetEntity>();
            ne.Id = id;
            ne.Epoch = epoch;
            ne.RemoteDestroyed = false;
            byId[id] = ne;
            return ne;
        }

        public static Vector2? PositionOf(uint id) {
            return byId.TryGetValue(id, out var ne) && ne != null ? (Vector2?)ne.transform.position : null;
        }

        public static uint FirstOfType(string typeName) {
            foreach (var kv in byId) {
                if (kv.Value != null && kv.Value.GetComponent(typeName) != null) {
                    return kv.Key;
                }
            }
            return 0;
        }

        public static bool DestroyLocal(uint id) {
            if (!byId.TryGetValue(id, out var ne) || ne == null) {
                return false;
            }
            UnityEngine.Object.Destroy(ne.gameObject);
            return true;
        }

        static NetEntity EntityOf(object instance) {
            if (!(instance is Component c) || c == null) {
                return null;
            }
            var ne = c.GetComponent<NetEntity>() ?? c.GetComponentInParent<NetEntity>();
            return ne != null && ne.Epoch == epoch ? ne : null;
        }

        static void StartPrefix(Component __instance, out CallState __state) {
            __state = default;
            var go = __instance.gameObject;
            var p = go.transform.position;
            var h = Salt;
            h = (h ^ Fnv(__instance.GetType().Name)) * 16777619u;
            h = (h ^ (uint)Mathf.RoundToInt(p.x * 10f)) * 16777619u;
            h = (h ^ (uint)Mathf.RoundToInt(p.y * 10f)) * 16777619u;
            h = (h ^ Fnv(go.name)) * 16777619u;
            __state.Seeded = true;
            __state.Outer = UnityEngine.Random.state;
            UnityEngine.Random.InitState((int)h);
        }

        static uint Fnv(string s) {
            var h = 2166136261u;
            foreach (var ch in s) {
                h = (h ^ ch) * 16777619u;
            }
            return h;
        }

        static void StartPostfix(CallState __state) {
            if (__state.Seeded) {
                UnityEngine.Random.state = __state.Outer;
            }
        }

        static bool RpcPrefix(object __instance, object[] __args, MethodBase __originalMethod, out CallState __state) {
            __state = default;
            if (applying || !rpcByMethod.TryGetValue(__originalMethod, out var rpc)) {
                return true;
            }
            var col = rpc.Mode == Mode.Trigger && __args.Length > 0 ? __args[0] as Collider2D : null;
            var owner = col != null ? col.GetComponentInParent<Body>() : null;
            if (owner != null && RemoteGate.IsRemote(owner)) {
                return false;
            }
            var ent = EntityOf(__instance);
            if (!session() || ent == null) {
                return true;
            }
            var host = isHost();
            var local = owner != null && PlayerCamera.main != null && owner == PlayerCamera.main.body;
            if (rpc.Mode == Mode.HostAction && !host) {
                return false;
            }
            if (rpc.Mode == Mode.Trigger && !host && !local) {
                return false;
            }
            if (rpc.Mode == Mode.Trigger) {
                var key = ((ulong)ent.Id << 16) | rpc.Index;
                var now = Time.time;
                if (lastTrigger.TryGetValue(key, out var last) && now - last < TriggerThrottle) {
                    return host;
                }
                lastTrigger[key] = now;
            }
            if (!Encode(rpc, __args, out var bytes)) {
                return host;
            }
            var seed = (uint)seeds.Next();
            Ct.EntityCall(ent.Id, rpc.Index, rpc.Persist ? FlagPersist : (byte)0, seed, bytes);
            if (!host) {
                return false;
            }
            __state.Seeded = true;
            __state.Outer = UnityEngine.Random.state;
            UnityEngine.Random.InitState((int)seed);
            return true;
        }

        static void RpcPostfix(CallState __state) {
            if (__state.Seeded) {
                UnityEngine.Random.state = __state.Outer;
            }
        }

        static IEnumerable<CodeInstruction> ContextTranspiler(IEnumerable<CodeInstruction> instructions) {
            var main = AccessTools.Field(typeof(PlayerCamera), nameof(PlayerCamera.main));
            var gate = AccessTools.Method(typeof(RemoteGate), nameof(RemoteGate.CameraForContext));
            foreach (var ins in instructions) {
                yield return ins;
                if (ins.opcode == OpCodes.Ldsfld && Equals(ins.operand, main)) {
                    yield return new CodeInstruction(OpCodes.Call, gate);
                }
            }
        }

        public static void OnDestroyed(NetEntity ne) {
            byId.Remove(ne.Id);
            if (ne.RemoteDestroyed || applying || quitting || ne.Epoch != epoch || !session()) {
                return;
            }
            var w = WorldGeneration.world;
            if (w == null || w.generatingWorld || (w.loadingObject != null && w.loadingObject.activeSelf) || !ne.gameObject.scene.isLoaded) {
                return;
            }
            Ct.EntityCall(ne.Id, MethodDestroy, 0, 0, null);
        }

        public static void ApplyRemote() {
            int len;
            while ((len = Ct.ct_entity_poll(pollBuf, pollBuf.Length)) > 0) {
                var id = BitConverter.ToUInt32(pollBuf, 0);
                var method = BitConverter.ToUInt16(pollBuf, 4);
                var seed = BitConverter.ToUInt32(pollBuf, 7);
                var argLen = BitConverter.ToUInt16(pollBuf, 11);
                if (method == CprMinigame.OpPump) {
                    CprMinigame.ApplyPump(pollBuf, 13, argLen);
                    continue;
                }
                if (Medical.Handles(method)) {
                    Medical.Apply(pollBuf, 13, argLen);
                    continue;
                }
                if (Enemies.Handles(method)) {
                    Enemies.Apply(id, method, pollBuf, 13, argLen);
                    continue;
                }
                if (method == Health.OpDamage) {
                    Health.ApplyDamage(pollBuf, 13, argLen);
                    continue;
                }
                if (Items.Handles(id)) {
                    Items.Apply(id, method, pollBuf, 13, argLen);
                    continue;
                }
                if (!byId.TryGetValue(id, out var ent) || ent == null) {
                    continue;
                }
                if (method == MethodDestroy) {
                    if (Together.Verbose) {
                        log.Debug("remote destroy " + Describe(id));
                    }
                    ent.RemoteDestroyed = true;
                    UnityEngine.Object.Destroy(ent.gameObject);
                    continue;
                }
                if (method >= rpcs.Count) {
                    continue;
                }
                var rpc = rpcs[method];
                var target = ent.GetComponentInChildren(rpc.Method.DeclaringType);
                if (target == null || !Decode(rpc, pollBuf, 13, argLen, out var args, out var context)) {
                    continue;
                }
                if (Together.Verbose) {
                    log.Debug($"remote {rpc.Method.DeclaringType.Name}.{rpc.Method.Name} on {Describe(id)}" + (context != null ? " (avatar)" : ""));
                }
                var outer = UnityEngine.Random.state;
                applying = true;
                RemoteGate.Context = context;
                try {
                    UnityEngine.Random.InitState((int)seed);
                    rpc.Method.Invoke(target, args);
                } catch (Exception e) {
                    log.Warning($"remote {rpc.Method.DeclaringType.Name}.{rpc.Method.Name} on #{id} failed: {e.InnerException?.Message ?? e.Message}");
                } finally {
                    RemoteGate.Context = null;
                    applying = false;
                    UnityEngine.Random.state = outer;
                }
            }
        }

        static bool Encode(Rpc rpc, object[] args, out byte[] bytes) {
            bytes = null;
            using (var ms = new MemoryStream()) {
                using (var w = new BinaryWriter(ms)) {
                    for (var i = 0; i < rpc.Params.Length; i++) {
                        if (!Write(w, rpc.Params[i].ParameterType, args[i])) {
                            return false;
                        }
                    }
                }
                bytes = ms.ToArray();
                return true;
            }
        }

        static bool Write(BinaryWriter w, Type t, object v) {
            if (t == typeof(int)) w.Write((int)v);
            else if (t == typeof(float)) w.Write((float)v);
            else if (t == typeof(bool)) w.Write((bool)v);
            else if (t == typeof(string)) w.Write((string)v ?? "");
            else if (t == typeof(Vector2)) { var p = (Vector2)v; w.Write(p.x); w.Write(p.y); }
            else if (t == typeof(Vector3)) { var p = (Vector3)v; w.Write(p.x); w.Write(p.y); w.Write(p.z); }
            else if (typeof(Collider2D).IsAssignableFrom(t)) return WriteCollider(w, v as Collider2D);
            else return false;
            return true;
        }

        static bool WriteCollider(BinaryWriter w, Collider2D c) {
            if (c == null) {
                return false;
            }
            var limb = c.GetComponentInParent<Limb>();
            var body = limb != null ? limb.body : c.GetComponentInParent<Body>();
            if (body != null) {
                var pid = PlayerIdOf(body);
                if (pid < 0) {
                    return false;
                }
                if (limb != null && c.gameObject == limb.gameObject) {
                    w.Write((byte)1);
                    w.Write((ushort)pid);
                    w.Write((byte)Array.IndexOf(body.limbs, limb));
                    w.Write((byte)Array.IndexOf(limb.GetComponents<Collider2D>(), c));
                    return true;
                }
                if (c.gameObject == body.gameObject) {
                    w.Write((byte)2);
                    w.Write((ushort)pid);
                    w.Write((byte)Array.IndexOf(body.GetComponents<Collider2D>(), c));
                    return true;
                }
                return false;
            }
            var ne = c.GetComponentInParent<NetEntity>();
            if (ne != null && ne.Epoch == epoch) {
                w.Write((byte)3);
                w.Write(ne.Id);
                w.Write((short)Array.IndexOf(ne.GetComponentsInChildren<Collider2D>(), c));
                return true;
            }
            return false;
        }

        static bool Decode(Rpc rpc, byte[] buf, int offset, int len, out object[] args, out Body context) {
            args = new object[rpc.Params.Length];
            context = null;
            using (var r = new BinaryReader(new MemoryStream(buf, offset, len))) {
                try {
                    for (var i = 0; i < args.Length; i++) {
                        var t = rpc.Params[i].ParameterType;
                        if (t == typeof(int)) args[i] = r.ReadInt32();
                        else if (t == typeof(float)) args[i] = r.ReadSingle();
                        else if (t == typeof(bool)) args[i] = r.ReadBoolean();
                        else if (t == typeof(string)) args[i] = r.ReadString();
                        else if (t == typeof(Vector2)) args[i] = new Vector2(r.ReadSingle(), r.ReadSingle());
                        else if (t == typeof(Vector3)) args[i] = new Vector3(r.ReadSingle(), r.ReadSingle(), r.ReadSingle());
                        else if (typeof(Collider2D).IsAssignableFrom(t)) {
                            var c = ReadCollider(r, out var owner);
                            if (c == null) {
                                return false;
                            }
                            if (owner != null && RemoteGate.IsRemote(owner)) {
                                context = owner;
                            }
                            args[i] = c;
                        } else {
                            return false;
                        }
                    }
                } catch (EndOfStreamException) {
                    return false;
                }
            }
            return true;
        }

        static Collider2D ReadCollider(BinaryReader r, out Body owner) {
            owner = null;
            switch (r.ReadByte()) {
                case 1: {
                    var body = BodyOfPlayer(r.ReadUInt16());
                    var li = r.ReadByte();
                    var ci = r.ReadByte();
                    if (body == null || li >= body.limbs.Length) return null;
                    owner = body;
                    var cols = body.limbs[li].GetComponents<Collider2D>();
                    return ci < cols.Length ? cols[ci] : null;
                }
                case 2: {
                    var body = BodyOfPlayer(r.ReadUInt16());
                    var ci = r.ReadByte();
                    if (body == null) return null;
                    owner = body;
                    var cols = body.GetComponents<Collider2D>();
                    return ci < cols.Length ? cols[ci] : null;
                }
                case 3: {
                    var id = r.ReadUInt32();
                    var ci = r.ReadInt16();
                    if (!byId.TryGetValue(id, out var ne) || ne == null || ci < 0) return null;
                    var cols = ne.GetComponentsInChildren<Collider2D>();
                    return ci < cols.Length ? cols[ci] : null;
                }
                default:
                    return null;
            }
        }

        internal static int PlayerIdOf(Body body) {
            if (PlayerCamera.main != null && body == PlayerCamera.main.body) {
                return Ct.ct_local_id();
            }
            return Avatars.IdOf(body);
        }

        internal static Container ContainerOf(uint id) {
            return byId.TryGetValue(id, out var ne) && ne != null ? ne.GetComponentInChildren<Container>() : null;
        }

        internal static Body BodyOfPlayer(ushort id) {
            if (id == Ct.ct_local_id()) {
                return PlayerCamera.main != null ? PlayerCamera.main.body : null;
            }
            return Avatars.BodyOf(id);
        }
    }
}

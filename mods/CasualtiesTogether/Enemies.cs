using System;
using System.Collections.Generic;
using System.IO;
using System.Reflection;
using System.Reflection.Emit;
using HarmonyLib;
using UnityEngine;

namespace CasualtiesTogether {
    static class Enemies {
        public const byte MotionKind = 3;
        const ushort OpSpawn = 0xF200;
        const ushort OpDamage = 0xF201;
        const uint RuntimeBase = 0x20000000;
        const float StreamInterval = 0.1f;
        const float StreamRadius = 100f;
        const float ScopeRadius = 40f;
        const int PerPacket = 30;

        static readonly Type[] AiTypes = {
            typeof(GuardScript), typeof(LaserDroneScript), typeof(SpiderHandler), typeof(CrystalEnemy),
            typeof(ElderThornbackBehaviour), typeof(ScrapEaterScript), typeof(TechballScript), typeof(BloodSuckerScript),
            typeof(GrabberPlant), typeof(XalorisScript)
        };

        static readonly Type[] SpawnerTypes = { typeof(GuardSpawner), typeof(CaveTickSpawner) };

        static readonly string[] Frozen = {
            "Update", "FixedUpdate", "LateUpdate", "OnCollisionEnter2D", "OnCollisionStay2D", "OnTriggerEnter2D", "OnTriggerStay2D"
        };

        struct Target {
            public Vector2 Pos;
            public Vector2 Vel;
            public float Rot;
            public float Health;
            public int Anim;
            public float Time;
        }

        static readonly HashSet<Type> aiSet = new HashSet<Type>(AiTypes);
        static readonly List<Vector2> avatarPos = new List<Vector2>();
        static readonly Dictionary<uint, Target> targets = new Dictionary<uint, Target>();
        static readonly Dictionary<BuildingEntity, float> buildingBefore = new Dictionary<BuildingEntity, float>();
        static readonly List<(ushort, Body)> avatars = new List<(ushort, Body)>();
        static readonly List<uint> scratch = new List<uint>();
        static readonly MemoryStream stream = new MemoryStream(1200);

        static ModLoader.Logger log;
        static Func<bool> session;
        static Func<bool> isHost;
        static uint counter;
        static float nextStream;

        public static bool Handles(ushort method) => method == OpSpawn || method == OpDamage;

        static bool Client => session() && !isHost();
        static bool Host => session() && isHost();

        public static void Install(Harmony harmony, ModLoader.Logger logger, Func<bool> inSession, Func<bool> host) {
            log = logger;
            session = inSession;
            isHost = host;
            var transpiler = new HarmonyMethod(typeof(Enemies), nameof(TargetTranspiler));
            var freezePre = new HarmonyMethod(typeof(Enemies), nameof(FreezePrefix)) { priority = Priority.First };
            var freezePost = new HarmonyMethod(typeof(Enemies), nameof(FreezePostfix));
            var start = new HarmonyMethod(typeof(Enemies), nameof(StartPostfix));
            int retargeted = 0, frozen = 0;
            foreach (var t in AiTypes) {
                foreach (var m in AccessTools.GetDeclaredMethods(t)) {
                    if (!m.IsStatic && m.GetMethodBody() != null && UsesPlayerBody(m)) {
                        harmony.Patch(m, transpiler: transpiler);
                        retargeted++;
                    }
                }
                foreach (var nested in t.GetNestedTypes(BindingFlags.NonPublic | BindingFlags.Public)) {
                    var mn = AccessTools.Method(nested, "MoveNext");
                    if (mn != null && AccessTools.Field(nested, "<>4__this")?.FieldType == t && UsesPlayerBody(mn)) {
                        harmony.Patch(mn, transpiler: transpiler);
                        retargeted++;
                    }
                }
            }
            foreach (var t in AiTypes) {
                foreach (var name in Frozen) {
                    var m = AccessTools.DeclaredMethod(t, name);
                    if (m != null) {
                        harmony.Patch(m, prefix: freezePre, postfix: freezePost);
                        frozen++;
                    }
                }
                var st = AccessTools.DeclaredMethod(t, "Start");
                if (st != null) {
                    harmony.Patch(st, postfix: start);
                }
            }
            foreach (var t in SpawnerTypes) {
                var m = AccessTools.DeclaredMethod(t, "Update");
                if (m != null) {
                    harmony.Patch(m, prefix: new HarmonyMethod(typeof(Enemies), nameof(SpawnerPrefix)));
                    frozen++;
                }
            }
            log.Info($"enemy sync: {AiTypes.Length} AI types, {retargeted} methods retargeted, {frozen} hooks host-only");
        }

        static readonly FieldInfo camMain = AccessTools.Field(typeof(PlayerCamera), nameof(PlayerCamera.main));
        static readonly FieldInfo camBody = AccessTools.Field(typeof(PlayerCamera), nameof(PlayerCamera.body));

        static bool UsesPlayerBody(MethodBase m) {
            try {
                foreach (var ins in PatchProcessor.GetOriginalInstructions(m)) {
                    if (ins.opcode == OpCodes.Ldfld && Equals(ins.operand, camBody)) {
                        return true;
                    }
                }
            } catch {
            }
            return false;
        }

        static IEnumerable<CodeInstruction> TargetTranspiler(IEnumerable<CodeInstruction> instructions, MethodBase original) {
            var owner = original.DeclaringType;
            var self = aiSet.Contains(owner) ? null : AccessTools.Field(owner, "<>4__this");
            var target = AccessTools.Method(typeof(Enemies), nameof(TargetFor));
            var list = new List<CodeInstruction>(instructions);
            for (var i = 0; i < list.Count; i++) {
                var ins = list[i];
                if (ins.opcode == OpCodes.Ldfld && Equals(ins.operand, camBody) && i > 0 && list[i - 1].opcode == OpCodes.Ldsfld && Equals(list[i - 1].operand, camMain)) {
                    var first = new CodeInstruction(OpCodes.Ldarg_0);
                    first.labels.AddRange(ins.labels);
                    first.blocks.AddRange(ins.blocks);
                    yield return first;
                    if (self != null) {
                        yield return new CodeInstruction(OpCodes.Ldfld, self);
                    }
                    yield return new CodeInstruction(OpCodes.Call, target);
                    continue;
                }
                yield return ins;
            }
        }

        public static Body TargetFor(PlayerCamera cam, Component self) {
            var local = cam != null ? cam.body : null;
            if (self == null || !Host) {
                return local;
            }
            Vector2 pos = self.transform.position;
            var best = local != null && local.alive ? local : null;
            var bestD = best != null ? ((Vector2)best.transform.position - pos).sqrMagnitude : float.MaxValue;
            Avatars.Collect(avatars);
            foreach (var (_, b) in avatars) {
                if (b == null || !b.alive) {
                    continue;
                }
                var d = ((Vector2)b.transform.position - pos).sqrMagnitude;
                if (d < bestD) {
                    bestD = d;
                    best = b;
                }
            }
            return best ?? local;
        }

        static bool IsDead(Component c) {
            var bld = c.GetComponent<BuildingEntity>();
            return bld != null && bld.health <= 0f;
        }

        static bool AvatarNear(Vector2 pos, float radius) {
            Avatars.Collect(avatars);
            var r2 = radius * radius;
            foreach (var (_, b) in avatars) {
                if (b != null && ((Vector2)b.transform.position - pos).sqrMagnitude < r2) {
                    return true;
                }
            }
            return false;
        }

        static bool FreezePrefix(Component __instance, out int __state) {
            __state = 0;
            if (Client) {
                if (!IsDead(__instance)) {
                    return false;
                }
                if (!Items.Muted) {
                    Items.Muted = true;
                    __state = 1;
                }
                return true;
            }
            if (Host && AvatarNear(__instance.transform.position, ScopeRadius)) {
                Health.Begin();
                __state = 2;
            }
            return true;
        }

        static void FreezePostfix(int __state) {
            if (__state == 1) {
                Items.Muted = false;
            } else if (__state == 2) {
                Health.End();
            }
        }

        static bool SpawnerPrefix() {
            return !Client;
        }

        static void StartPostfix(Component __instance) {
            if (!session() || !Avatars.InWorld) {
                return;
            }
            var go = __instance.gameObject;
            if (go.GetComponent<NetEntity>() != null) {
                return;
            }
            if (Client) {
                UnityEngine.Object.Destroy(go);
                return;
            }
            var id = RuntimeBase | (++counter & 0x0FFFFFFF);
            Entities.AddRuntime(go, id);
            var prefab = go.name.Replace("(Clone)", "").Trim();
            using (var ms = new MemoryStream()) {
                using (var w = new BinaryWriter(ms)) {
                    w.Write(prefab);
                    w.Write(go.transform.position.x);
                    w.Write(go.transform.position.y);
                    w.Write(go.transform.eulerAngles.z);
                }
                Ct.EntityCall(id, OpSpawn, 1, 0, ms.ToArray());
            }
            if (Together.Verbose) {
                log.Debug($"enemy spawned #{id:X8} {prefab}");
            }
        }

        public static void Apply(uint id, ushort method, byte[] buf, int offset, int len) {
            using (var r = new BinaryReader(new MemoryStream(buf, offset, len))) {
                try {
                    if (method == OpSpawn) {
                        if ((id & 0xF0000000) == RuntimeBase && (id & 0x0FFFFFFF) > counter) {
                            counter = id & 0x0FFFFFFF;
                        }
                        if (!Client || Entities.ById(id) != null) {
                            return;
                        }
                        var prefab = r.ReadString();
                        var pos = new Vector2(r.ReadSingle(), r.ReadSingle());
                        var rot = r.ReadSingle();
                        GameObject go;
                        Items.Muted = true;
                        try {
                            go = Utils.Create(prefab, pos, rot);
                        } finally {
                            Items.Muted = false;
                        }
                        if (go != null) {
                            Entities.AddRuntime(go, id);
                            if (Together.Verbose) {
                                log.Debug($"enemy mirrored #{id:X8} {prefab}");
                            }
                        } else {
                            log.Warning($"cannot spawn enemy '{prefab}'");
                        }
                        return;
                    }
                    if (method == OpDamage && Host) {
                        var ne = Entities.ById(id);
                        var bld = ne != null ? ne.GetComponent<BuildingEntity>() : null;
                        var delta = r.ReadSingle();
                        if (bld != null) {
                            bld.health += delta;
                            if (Together.Verbose) {
                                log.Debug($"player damage {delta:0.0} to #{id:X8} {ne.name}, health now {bld.health:0.0}");
                            }
                        }
                    }
                } catch (EndOfStreamException) {
                }
            }
        }

        public static void BeginPlayerScope() {
            buildingBefore.Clear();
            if (!Client) {
                return;
            }
            var body = PlayerCamera.main != null ? PlayerCamera.main.body : null;
            if (body == null) {
                return;
            }
            Vector2 p = body.transform.position;
            foreach (var ne in Entities.Values) {
                if (ne == null || ((Vector2)ne.transform.position - p).sqrMagnitude > ScopeRadius * ScopeRadius) {
                    continue;
                }
                var bld = ne.Building();
                if (bld != null) {
                    buildingBefore[bld] = bld.health;
                }
            }
        }

        public static void EndPlayerScope() {
            if (buildingBefore.Count == 0) {
                return;
            }
            foreach (var kv in buildingBefore) {
                var bld = kv.Key;
                if (bld == null) {
                    continue;
                }
                var delta = bld.health - kv.Value;
                if (delta < -0.01f) {
                    var ne = bld.GetComponent<NetEntity>();
                    if (ne != null) {
                        var bytes = BitConverter.GetBytes(delta);
                        Ct.EntityCall(ne.Id, OpDamage, 0, 0, bytes);
                    }
                }
            }
            buildingBefore.Clear();
        }

        public static void Update() {
            if (!Host || Time.time < nextStream) {
                return;
            }
            nextStream = Time.time + StreamInterval;
            Avatars.Collect(avatars);
            var local = PlayerCamera.main != null ? PlayerCamera.main.body : null;
            var r2 = StreamRadius * StreamRadius;
            var count = 0;
            BinaryWriter w = null;
            var hasLocal = local != null;
            Vector2 lp = hasLocal ? (Vector2)local.transform.position : Vector2.zero;
            avatarPos.Clear();
            foreach (var (_, b) in avatars) {
                if (b != null) {
                    avatarPos.Add(b.transform.position);
                }
            }
            foreach (var ne in Entities.Values) {
                if (ne == null || ne.EnemyKind == 1) {
                    continue;
                }
                if (ne.EnemyKind == 0) {
                    ne.EnemyKind = 1;
                    foreach (var t in AiTypes) {
                        if (ne.GetComponent(t) != null) {
                            ne.EnemyKind = 2;
                            break;
                        }
                    }
                    if (ne.EnemyKind == 1) {
                        continue;
                    }
                    ne.Rb = ne.GetComponent<Rigidbody2D>();
                    ne.Anim = ne.GetComponentInChildren<Animator>();
                }
                Vector2 p = ne.transform.position;
                var near = hasLocal && (lp - p).sqrMagnitude < r2;
                for (var i = 0; !near && i < avatarPos.Count; i++) {
                    near = (avatarPos[i] - p).sqrMagnitude < r2;
                }
                if (!near) {
                    continue;
                }
                if (w == null) {
                    stream.SetLength(0);
                    w = new BinaryWriter(stream);
                    w.Write(MotionKind);
                    w.Write((byte)0);
                }
                var rb = ne.Rb;
                var bld = ne.Building();
                var anim = ne.Anim;
                w.Write(ne.Id);
                w.Write(p.x);
                w.Write(p.y);
                w.Write(ne.transform.eulerAngles.z);
                w.Write(rb != null ? rb.velocity.x : 0f);
                w.Write(rb != null ? rb.velocity.y : 0f);
                w.Write(bld != null ? bld.health : float.NaN);
                w.Write(anim != null && anim.isActiveAndEnabled ? anim.GetCurrentAnimatorStateInfo(0).fullPathHash : 0);
                if (++count == PerPacket) {
                    Flush(w, count);
                    w = null;
                    count = 0;
                }
            }
            if (w != null && count > 0) {
                Flush(w, count);
            }
        }

        static void Flush(BinaryWriter w, int count) {
            w.Flush();
            var bytes = stream.GetBuffer();
            bytes[1] = (byte)count;
            Ct.MotionSend(bytes, (int)stream.Length);
        }

        public static void OnStream(byte[] buf, int len) {
            if (!Client || len < 2) {
                return;
            }
            var count = buf[1];
            var pos = 2;
            var now = Time.time;
            for (var i = 0; i < count && pos + 32 <= len; i++) {
                var id = BitConverter.ToUInt32(buf, pos);
                targets[id] = new Target {
                    Pos = new Vector2(BitConverter.ToSingle(buf, pos + 4), BitConverter.ToSingle(buf, pos + 8)),
                    Rot = BitConverter.ToSingle(buf, pos + 12),
                    Vel = new Vector2(BitConverter.ToSingle(buf, pos + 16), BitConverter.ToSingle(buf, pos + 20)),
                    Health = BitConverter.ToSingle(buf, pos + 24),
                    Anim = BitConverter.ToInt32(buf, pos + 28),
                    Time = now
                };
                pos += 32;
            }
        }

        public static void FixedUpdate() {
            if (targets.Count == 0) {
                return;
            }
            if (!Client) {
                targets.Clear();
                return;
            }
            var now = Time.time;
            scratch.Clear();
            foreach (var kv in targets) {
                var ne = Entities.ById(kv.Key);
                if (ne == null || now - kv.Value.Time > 2f) {
                    scratch.Add(kv.Key);
                    continue;
                }
                var t = kv.Value;
                var goal = t.Pos + t.Vel * Mathf.Min(now - t.Time, 0.2f);
                var rb = ne.GetComponent<Rigidbody2D>();
                if (rb != null && rb.simulated && rb.bodyType == RigidbodyType2D.Dynamic) {
                    var err = goal - rb.position;
                    if (err.sqrMagnitude > 16f) {
                        rb.position = goal;
                        rb.velocity = t.Vel;
                    } else {
                        rb.velocity = t.Vel + err * 8f;
                    }
                    rb.MoveRotation(Mathf.LerpAngle(rb.rotation, t.Rot, 0.3f));
                } else {
                    ne.transform.position = Vector2.Lerp(ne.transform.position, goal, 0.35f);
                    ne.transform.rotation = Quaternion.Euler(0f, 0f, Mathf.LerpAngle(ne.transform.eulerAngles.z, t.Rot, 0.35f));
                }
                var bld = ne.GetComponent<BuildingEntity>();
                if (bld != null && !float.IsNaN(t.Health) && Mathf.Abs(bld.health - t.Health) > 0.01f) {
                    bld.health = t.Health;
                }
                if (t.Anim != 0) {
                    var anim = ne.GetComponentInChildren<Animator>();
                    if (anim != null && anim.isActiveAndEnabled && anim.GetCurrentAnimatorStateInfo(0).fullPathHash != t.Anim) {
                        anim.Play(t.Anim, 0);
                    }
                }
            }
            foreach (var id in scratch) {
                targets.Remove(id);
            }
        }

        public static void Reset() {
            targets.Clear();
            buildingBefore.Clear();
        }
    }
}

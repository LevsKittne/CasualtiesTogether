using System;
using System.Collections.Generic;
using System.IO;
using HarmonyLib;
using UnityEngine;

namespace CasualtiesTogether {
    sealed class NetItem : MonoBehaviour {
        public uint Id;
        public bool RemoteDestroyed;
        public bool Authority;
        public float RestTime;
        public string Signature;
    }

    static class Items {
        public const uint GenBase = 0x40000000;
        const uint RuntimeBit = 0x80000000;
        const ushort OpUpsert = 0xF001;
        const ushort OpDestroy = 0xFFFF;
        const byte FlagPersist = 1;
        const byte FlagReplace = 2;
        const float MotionInterval = 0.1f;
        const float RefreshInterval = 1f;
        const float FullRefreshInterval = 10f;

        enum Holder : byte {
            World = 0,
            Slot = 1,
            Wear = 2,
            ItemContainer = 3,
            EntityContainer = 4
        }

        struct MotionTarget {
            public Vector2 Pos;
            public Vector2 Vel;
            public float Rot;
            public float Ang;
            public float Time;
        }

        sealed class Pending {
            public byte[] Data;
            public float Retry;
        }

        static readonly Dictionary<uint, Item> byId = new Dictionary<uint, Item>();
        static readonly HashSet<Item> dirty = new HashSet<Item>();
        static readonly Dictionary<int, Vector2> spawnPos = new Dictionary<int, Vector2>();
        static readonly Dictionary<uint, MotionTarget> motion = new Dictionary<uint, MotionTarget>();
        static readonly Dictionary<uint, Pending> pending = new Dictionary<uint, Pending>();
        static readonly List<uint> scratch = new List<uint>();
        static readonly List<Item> scratchItems = new List<Item>();
        static readonly byte[] motionBuf = new byte[1200];
        static readonly MemoryStream motionStream = new MemoryStream(1200);

        static ModLoader.Logger log;
        static Func<bool> session;
        static uint counter;
        static uint genCount;
        static bool applying;
        static bool quitting;
        static float nextMotion;
        static float nextRefresh;
        static float nextFullRefresh;
        static bool forceRefresh;
        static int warnedPrefab;

        public static int Count => byId.Count;
        public static bool Muted;
        static readonly HashSet<int> localOnly = new HashSet<int>();

        public static void Install(Harmony harmony, ModLoader.Logger logger, Func<bool> inSession) {
            log = logger;
            session = inSession;
            void Post(Type t, string name, string patch, Type[] args = null) {
                var m = args == null ? AccessTools.DeclaredMethod(t, name) : AccessTools.DeclaredMethod(t, name, args);
                if (m == null) {
                    log.Warning($"item sync: {t.Name}.{name} not found");
                    return;
                }
                harmony.Patch(m, postfix: new HarmonyMethod(typeof(Items), patch));
            }
            void Pre(Type t, string name, string patch, Type[] args = null) {
                var m = args == null ? AccessTools.DeclaredMethod(t, name) : AccessTools.DeclaredMethod(t, name, args);
                if (m == null) {
                    log.Warning($"item sync: {t.Name}.{name} not found");
                    return;
                }
                harmony.Patch(m, prefix: new HarmonyMethod(typeof(Items), patch));
            }
            Post(typeof(Item), "Awake", nameof(AwakePostfix));
            Post(typeof(Item), "Start", nameof(StartPostfix));
            Pre(typeof(Item), "OnDestroy", nameof(DestroyPrefix));
            Post(typeof(Body), nameof(Body.PickUpItem), nameof(BodyItemPostfix), new[] { typeof(Item), typeof(int), typeof(bool) });
            Post(typeof(Body), nameof(Body.DropItem), nameof(BodyItemPostfix), new[] { typeof(Item) });
            Post(typeof(Body), nameof(Body.WearWearable), nameof(BodyItemPostfix), new[] { typeof(Item) });
            Post(typeof(Body), nameof(Body.DropWearable), nameof(BodyItemPostfix), new[] { typeof(Item) });
            Post(typeof(Body), nameof(Body.AutoPickUpItem), nameof(BodyItemPostfix), new[] { typeof(Item) });
            var throwM = AccessTools.DeclaredMethod(typeof(Body), nameof(Body.ThrowItem));
            harmony.Patch(throwM, prefix: new HarmonyMethod(typeof(Items), nameof(ThrowPrefix)), postfix: new HarmonyMethod(typeof(Items), nameof(ThrowPostfix)));
            Post(typeof(Container), nameof(Container.LoadItem), nameof(ContainerPostfix), new[] { typeof(Item) });
            Post(typeof(Container), nameof(Container.UnloadItem), nameof(ContainerPostfix), new[] { typeof(Item), typeof(Body) });
            log.Info("item sync ready");
        }

        public static void OnQuit() {
            quitting = true;
        }

        static Body LocalBody => PlayerCamera.main != null ? PlayerCamera.main.body : null;

        static bool OwnedByLocal(Item it) {
            var lb = LocalBody;
            return lb != null && it.GetComponentInParent<Body>() == lb;
        }

        static bool UnderAvatar(Item it) {
            var b = it.GetComponentInParent<Body>();
            return b != null && RemoteGate.IsRemote(b);
        }

        public static void Register() {
            var keep = new List<Item>();
            foreach (var kv in byId) {
                if (kv.Value != null && (kv.Key & RuntimeBit) != 0 && OwnedByLocal(kv.Value)) {
                    keep.Add(kv.Value);
                }
            }
            byId.Clear();
            dirty.Clear();
            motion.Clear();
            pending.Clear();
            genCount = 0;
            foreach (var it in keep) {
                byId[it.GetComponent<NetItem>().Id] = it;
            }
            AssignGenerated();
        }

        public static void RegisterLate(bool session, bool host) {
            var late = new List<Item>();
            foreach (var it in UnityEngine.Object.FindObjectsOfType<Item>()) {
                if (it.GetComponent<NetItem>() == null && !OwnedByLocal(it) && !UnderAvatar(it)) {
                    late.Add(it);
                }
            }
            spawnPos.Clear();
            if (session && !host) {
                foreach (var it in late) {
                    UnityEngine.Object.Destroy(it.gameObject);
                }
                log.Info($"removed {late.Count} local items spawned while the world was finishing, host copies will follow");
            } else {
                foreach (var it in late) {
                    EnsureId(it);
                    if (session) {
                        dirty.Add(it);
                    }
                }
                log.Info($"{late.Count} items spawned while the world was finishing" + (session ? ", shared" : ""));
            }
            forceRefresh = true;
        }

        public static void OnHostStarted() {
            foreach (var kv in byId) {
                if (kv.Value != null && (kv.Key & RuntimeBit) != 0 && !UnderAvatar(kv.Value)) {
                    dirty.Add(kv.Value);
                }
            }
            forceRefresh = true;
        }

        static bool IsMine(uint id) {
            return (id & RuntimeBit) != 0 && ((id >> 20) & 0x7FF) == ((uint)Math.Max(0, Ct.ct_local_id()) & 0x7FF);
        }

        static void AssignGenerated() {
            var list = new List<(string id, Vector2 pos, Item item)>();
            foreach (var it in UnityEngine.Object.FindObjectsOfType<Item>()) {
                if (it.GetComponent<NetItem>() != null || OwnedByLocal(it) || UnderAvatar(it)) {
                    continue;
                }
                var pos = spawnPos.TryGetValue(it.gameObject.GetInstanceID(), out var sp) ? sp : (Vector2)it.transform.position;
                list.Add((it.id ?? "", pos, it));
            }
            list.Sort((a, b) => {
                var c = string.CompareOrdinal(a.id, b.id);
                if (c != 0) return c;
                c = Mathf.RoundToInt(a.pos.x * 100f).CompareTo(Mathf.RoundToInt(b.pos.x * 100f));
                return c != 0 ? c : Mathf.RoundToInt(a.pos.y * 100f).CompareTo(Mathf.RoundToInt(b.pos.y * 100f));
            });
            foreach (var e in list) {
                var ni = e.item.gameObject.AddComponent<NetItem>();
                ni.Id = GenBase + ++genCount;
                byId[ni.Id] = e.item;
            }
            spawnPos.Clear();
            log.Info($"registered {list.Count} generated items ({byId.Count} tracked)");
        }

        public static void Reset() {
            dirty.Clear();
            motion.Clear();
            pending.Clear();
        }

        static uint EnsureId(Item it) {
            var ni = it.GetComponent<NetItem>();
            if (ni != null) {
                return ni.Id;
            }
            var pid = (uint)Math.Max(0, Ct.ct_local_id()) & 0x7FF;
            ni = it.gameObject.AddComponent<NetItem>();
            ni.Id = RuntimeBit | (pid << 20) | (++counter & 0xFFFFF);
            byId[ni.Id] = it;
            return ni.Id;
        }

        static void AwakePostfix(Item __instance) {
            spawnPos[__instance.gameObject.GetInstanceID()] = __instance.transform.position;
            if (Muted) {
                localOnly.Add(__instance.gameObject.GetInstanceID());
            }
        }

        static void StartPostfix(Item __instance) {
            if (localOnly.Remove(__instance.gameObject.GetInstanceID())) {
                return;
            }
            if (applying || !session() || !Avatars.InWorld || __instance.GetComponent<NetItem>() != null || UnderAvatar(__instance)) {
                return;
            }
            EnsureId(__instance);
            dirty.Add(__instance);
            if (__instance.transform.parent == null) {
                __instance.GetComponent<NetItem>().Authority = true;
            }
        }

        static void DestroyPrefix(Item __instance) {
            var ni = __instance.GetComponent<NetItem>();
            if (ni == null) {
                return;
            }
            if (byId.TryGetValue(ni.Id, out var cur) && cur == __instance) {
                byId.Remove(ni.Id);
            }
            if (ni.RemoteDestroyed || quitting || !session()) {
                return;
            }
            var w = WorldGeneration.world;
            if (w == null || w.generatingWorld || (w.loadingObject != null && w.loadingObject.activeSelf) || !__instance.gameObject.scene.isLoaded || UnderAvatar(__instance)) {
                return;
            }
            Ct.EntityCall(ni.Id, OpDestroy, 0, 0, null);
        }

        static void BodyItemPostfix(Body __instance, Item item) {
            Mark(__instance, item);
        }

        static void ThrowPrefix(Body __instance, out Item __state) {
            __state = __instance.GetItem(__instance.handSlot);
        }

        static void ThrowPostfix(Body __instance, Item __state) {
            Mark(__instance, __state);
        }

        static void ContainerPostfix(Item item) {
            if (applying || item == null || !session()) {
                return;
            }
            EnsureId(item);
            dirty.Add(item);
        }

        static void Mark(Body body, Item item) {
            if (applying || item == null || !session() || body != LocalBody) {
                return;
            }
            EnsureId(item);
            dirty.Add(item);
            item.GetComponent<NetItem>().Authority = item.transform.parent == null;
        }

        public static void LateUpdate() {
            if (!session() || dirty.Count == 0) {
                return;
            }
            scratchItems.Clear();
            scratchItems.AddRange(dirty);
            dirty.Clear();
            foreach (var it in scratchItems) {
                if (it != null) {
                    SendUpsert(it);
                }
            }
        }

        static void SendUpsert(Item it) {
            var ni = it.GetComponent<NetItem>();
            if (ni == null || !Encode(it, out var bytes, out var sig)) {
                return;
            }
            ni.Signature = sig;
            Ct.EntityCall(ni.Id, OpUpsert, FlagPersist | FlagReplace, 0, bytes);
        }

        static bool Encode(Item it, out byte[] bytes, out string signature) {
            using (var ms = new MemoryStream()) {
                using (var w = new BinaryWriter(ms)) {
                    w.Write(it.id ?? "");
                    w.Write(it.condition);
                    var p = it.transform.parent;
                    var slot = p != null ? p.GetComponent<InventorySlot>() : null;
                    var limb = p != null && slot == null ? p.GetComponent<Limb>() : null;
                    var cont = p != null && slot == null && limb == null ? p.GetComponent<Container>() : null;
                    signature = null;
                    if (slot != null && slot.body != null && Entities.PlayerIdOf(slot.body) >= 0) {
                        w.Write((byte)Holder.Slot);
                        w.Write((ushort)Entities.PlayerIdOf(slot.body));
                        w.Write((byte)slot.slot);
                        signature = $"s{Entities.PlayerIdOf(slot.body)}:{slot.slot}";
                    } else if (limb != null && limb.body != null && Entities.PlayerIdOf(limb.body) >= 0) {
                        w.Write((byte)Holder.Wear);
                        w.Write((ushort)Entities.PlayerIdOf(limb.body));
                        w.Write((byte)Array.IndexOf(limb.body.limbs, limb));
                        signature = $"w{Entities.PlayerIdOf(limb.body)}";
                    } else if (cont != null && cont.GetComponent<NetItem>() != null) {
                        w.Write((byte)Holder.ItemContainer);
                        w.Write(cont.GetComponent<NetItem>().Id);
                        signature = $"c{cont.GetComponent<NetItem>().Id}";
                    } else if (cont != null && cont.GetComponent<NetEntity>() != null) {
                        w.Write((byte)Holder.EntityContainer);
                        w.Write(cont.GetComponent<NetEntity>().Id);
                        signature = $"e{cont.GetComponent<NetEntity>().Id}";
                    } else {
                        w.Write((byte)Holder.World);
                        Vector2 pos = it.transform.position;
                        var vel = it.rb != null && it.rb.simulated ? it.rb.velocity : Vector2.zero;
                        w.Write(pos.x);
                        w.Write(pos.y);
                        w.Write(it.transform.eulerAngles.z);
                        w.Write(vel.x);
                        w.Write(vel.y);
                        w.Write(it.rb != null ? it.rb.angularVelocity : 0f);
                        signature = "world";
                    }
                    signature += ":" + Mathf.RoundToInt(it.condition * 20f);
                }
                bytes = ms.ToArray();
                return true;
            }
        }

        public static bool Handles(uint id) => id >= GenBase;

        public static void Apply(uint id, ushort method, byte[] buf, int offset, int len) {
            if (method == OpDestroy) {
                pending.Remove(id);
                if (byId.TryGetValue(id, out var dead) && dead != null) {
                    var ni = dead.GetComponent<NetItem>();
                    if (ni != null) {
                        ni.RemoteDestroyed = true;
                    }
                    byId.Remove(id);
                    UnityEngine.Object.Destroy(dead.gameObject);
                }
                return;
            }
            if (method != OpUpsert || (IsMine(id) && byId.TryGetValue(id, out var own) && own != null)) {
                return;
            }
            var data = new byte[len];
            Buffer.BlockCopy(buf, offset, data, 0, len);
            if (!ApplyUpsert(id, data)) {
                pending[id] = new Pending { Data = data, Retry = Time.time + 1f };
            } else {
                pending.Remove(id);
            }
        }

        static bool ApplyUpsert(uint id, byte[] data) {
            using (var r = new BinaryReader(new MemoryStream(data))) {
                string prefab;
                float condition;
                Holder kind;
                try {
                    prefab = r.ReadString();
                    condition = r.ReadSingle();
                    kind = (Holder)r.ReadByte();
                } catch (EndOfStreamException) {
                    return true;
                }
                if (Together.Verbose) {
                    log.Debug($"item #{id:X8} {prefab} -> {kind}");
                }
                byId.TryGetValue(id, out var it);
                if (it == null) {
                    it = Spawn(id, prefab);
                    if (it == null) {
                        return true;
                    }
                }
                applying = true;
                try {
                    if (Mathf.Abs(it.condition - condition) > 0.0001f) {
                        it.SetCondition(condition);
                    }
                    switch (kind) {
                        case Holder.World: {
                            var pos = new Vector2(r.ReadSingle(), r.ReadSingle());
                            var rot = r.ReadSingle();
                            var vel = new Vector2(r.ReadSingle(), r.ReadSingle());
                            var ang = r.ReadSingle();
                            Detach(it);
                            it.gameObject.SetActive(true);
                            it.transform.position = pos;
                            it.transform.rotation = Quaternion.Euler(0f, 0f, rot);
                            if (it.rb != null) {
                                it.rb.simulated = true;
                                it.rb.position = pos;
                                it.rb.velocity = vel;
                                it.rb.angularVelocity = ang;
                            }
                            it.GetComponent<NetItem>().Authority = false;
                            return true;
                        }
                        case Holder.Slot: {
                            var body = Entities.BodyOfPlayer(r.ReadUInt16());
                            var slot = r.ReadByte();
                            if (body == null || slot >= body.slots.Length) {
                                Hide(it);
                                return false;
                            }
                            var current = body.GetItem(slot);
                            if (current == it) {
                                return true;
                            }
                            Detach(it);
                            it.gameObject.SetActive(true);
                            if (current != null) {
                                body.DropItem(current);
                            }
                            it.transform.position = body.transform.position;
                            body.PickUpItem(it, slot, true);
                            return body.GetItem(slot) == it;
                        }
                        case Holder.Wear: {
                            var body = Entities.BodyOfPlayer(r.ReadUInt16());
                            r.ReadByte();
                            if (body == null) {
                                Hide(it);
                                return false;
                            }
                            if (body.HasWearable(it)) {
                                return true;
                            }
                            Detach(it);
                            it.gameObject.SetActive(true);
                            it.transform.position = body.transform.position;
                            body.WearWearable(it);
                            return body.HasWearable(it);
                        }
                        case Holder.ItemContainer:
                        case Holder.EntityContainer: {
                            var cid = r.ReadUInt32();
                            Container cont = null;
                            if (kind == Holder.ItemContainer) {
                                if (byId.TryGetValue(cid, out var ci) && ci != null) {
                                    cont = ci.GetComponent<Container>();
                                }
                            } else {
                                cont = Entities.ContainerOf(cid);
                            }
                            if (cont == null) {
                                Hide(it);
                                return false;
                            }
                            if (it.transform.parent == cont.transform) {
                                return true;
                            }
                            Detach(it);
                            it.gameObject.SetActive(true);
                            it.transform.position = cont.transform.position;
                            cont.LoadItem(it);
                            return it.transform.parent == cont.transform;
                        }
                    }
                } catch (EndOfStreamException) {
                    return true;
                } catch (Exception e) {
                    log.Warning($"item #{id:X8} upsert failed: {e.Message}");
                    return true;
                } finally {
                    applying = false;
                }
            }
            return true;
        }

        static void Hide(Item it) {
            Detach(it);
            it.gameObject.SetActive(false);
        }

        static void Detach(Item it) {
            var p = it.transform.parent;
            if (p == null) {
                return;
            }
            var slot = p.GetComponent<InventorySlot>();
            if (slot != null && slot.body != null) {
                slot.body.DropItem(it);
            } else {
                var limb = p.GetComponent<Limb>();
                if (limb != null && limb.body != null && limb.body.HasWearable(it)) {
                    limb.body.DropWearable(it);
                } else {
                    var cont = p.GetComponent<Container>();
                    if (cont != null) {
                        cont.UnloadItem(it);
                    }
                }
            }
            if (it.transform.parent != null) {
                it.transform.SetParent(null);
            }
        }

        static Item Spawn(uint id, string prefab) {
            if (string.IsNullOrEmpty(prefab)) {
                return null;
            }
            GameObject go;
            applying = true;
            try {
                go = Utils.Create(prefab, Vector2.zero, 0f);
            } catch {
                go = null;
            } finally {
                applying = false;
            }
            var it = go != null ? go.GetComponent<Item>() : null;
            if (it == null) {
                if (go != null) {
                    UnityEngine.Object.Destroy(go);
                }
                if (warnedPrefab++ < 5) {
                    log.Warning($"cannot spawn item '{prefab}' for #{id:X8}");
                }
                return null;
            }
            var ni = go.AddComponent<NetItem>();
            ni.Id = id;
            byId[id] = it;
            return it;
        }

        public static void Update() {
            if (!session()) {
                return;
            }
            var now = Time.time;
            if (pending.Count > 0) {
                scratch.Clear();
                foreach (var kv in pending) {
                    if (now >= kv.Value.Retry) {
                        scratch.Add(kv.Key);
                    }
                }
                foreach (var id in scratch) {
                    var p = pending[id];
                    if (ApplyUpsert(id, p.Data)) {
                        pending.Remove(id);
                    } else {
                        p.Retry = now + 1f;
                    }
                }
            }
            ReceiveMotion();
            if (now >= nextMotion) {
                nextMotion = now + MotionInterval;
                SendMotion();
            }
            if (now >= nextRefresh || forceRefresh) {
                var full = forceRefresh || now >= nextFullRefresh;
                if (full) {
                    nextFullRefresh = now + FullRefreshInterval;
                }
                forceRefresh = false;
                nextRefresh = now + RefreshInterval;
                Refresh(full);
            }
        }

        static void Refresh(bool full) {
            var body = LocalBody;
            if (body == null || !Avatars.InWorld) {
                return;
            }
            scratchItems.Clear();
            foreach (var it in body.GetComponentsInChildren<Item>(true)) {
                scratchItems.Add(it);
            }
            foreach (var it in scratchItems) {
                EnsureId(it);
                var ni = it.GetComponent<NetItem>();
                if (!full && ni.Signature != null && Encode(it, out _, out var sig) && sig == ni.Signature) {
                    continue;
                }
                SendUpsert(it);
            }
        }

        static void SendMotion() {
            motionStream.SetLength(0);
            var w = new BinaryWriter(motionStream);
            w.Write((byte)1);
            w.Write((byte)0);
            byte count = 0;
            var now = Time.time;
            scratchItems.Clear();
            foreach (var kv in byId) {
                var it = kv.Value;
                if (it == null) {
                    continue;
                }
                var ni = it.GetComponent<NetItem>();
                if (!ni.Authority) {
                    continue;
                }
                if (it.transform.parent != null || it.rb == null || !it.rb.simulated) {
                    ni.Authority = false;
                    continue;
                }
                if (it.rb.velocity.sqrMagnitude < 0.01f && Mathf.Abs(it.rb.angularVelocity) < 1f) {
                    if (ni.RestTime == 0f) {
                        ni.RestTime = now;
                    } else if (now - ni.RestTime > 0.5f) {
                        ni.Authority = false;
                        ni.RestTime = 0f;
                        scratchItems.Add(it);
                    }
                    continue;
                }
                ni.RestTime = 0f;
                if (count == 60) {
                    continue;
                }
                Vector2 p = it.transform.position;
                w.Write(kv.Key);
                w.Write(p.x);
                w.Write(p.y);
                w.Write(it.rb.velocity.x);
                w.Write(it.rb.velocity.y);
                w.Write(it.transform.eulerAngles.z);
                w.Write(it.rb.angularVelocity);
                count++;
            }
            foreach (var it in scratchItems) {
                SendUpsert(it);
            }
            if (count == 0) {
                return;
            }
            w.Flush();
            var bytes = motionStream.GetBuffer();
            bytes[1] = count;
            Ct.MotionSend(bytes, (int)motionStream.Length);
        }

        static void ReceiveMotion() {
            int len;
            var now = Time.time;
            while ((len = Ct.ct_motion_poll(motionBuf, motionBuf.Length)) > 0) {
                if (motionBuf[0] == Health.MotionKind) {
                    Health.OnVitals(motionBuf, len);
                    continue;
                }
                if (motionBuf[0] == Enemies.MotionKind) {
                    Enemies.OnStream(motionBuf, len);
                    continue;
                }
                if (motionBuf[0] != 1 || len < 2) {
                    continue;
                }
                var count = motionBuf[1];
                var pos = 2;
                for (var i = 0; i < count && pos + 28 <= len; i++) {
                    var id = BitConverter.ToUInt32(motionBuf, pos);
                    motion[id] = new MotionTarget {
                        Pos = new Vector2(BitConverter.ToSingle(motionBuf, pos + 4), BitConverter.ToSingle(motionBuf, pos + 8)),
                        Vel = new Vector2(BitConverter.ToSingle(motionBuf, pos + 12), BitConverter.ToSingle(motionBuf, pos + 16)),
                        Rot = BitConverter.ToSingle(motionBuf, pos + 20),
                        Ang = BitConverter.ToSingle(motionBuf, pos + 24),
                        Time = now
                    };
                    pos += 28;
                }
            }
        }

        public static void FixedUpdate() {
            if (motion.Count == 0) {
                return;
            }
            var now = Time.time;
            scratch.Clear();
            foreach (var kv in motion) {
                if (now - kv.Value.Time > 1f || !byId.TryGetValue(kv.Key, out var it) || it == null || it.rb == null || it.transform.parent != null) {
                    scratch.Add(kv.Key);
                    continue;
                }
                if (it.GetComponent<NetItem>().Authority) {
                    scratch.Add(kv.Key);
                    continue;
                }
                var t = kv.Value;
                var target = t.Pos + t.Vel * (now - t.Time);
                var err = target - it.rb.position;
                if (err.sqrMagnitude > 9f) {
                    it.rb.position = target;
                    it.rb.velocity = t.Vel;
                } else {
                    it.rb.velocity = t.Vel + err * 8f;
                }
                it.rb.angularVelocity = t.Ang + Mathf.DeltaAngle(it.rb.rotation, t.Rot) * 5f;
            }
            foreach (var id in scratch) {
                motion.Remove(id);
            }
        }
    }
}

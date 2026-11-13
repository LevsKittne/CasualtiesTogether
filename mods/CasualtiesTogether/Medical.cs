using System;
using System.Collections.Generic;
using System.IO;
using System.Reflection;
using HarmonyLib;
using UnityEngine;

namespace CasualtiesTogether {
    static class Medical {
        public const ushort OpTreat = 0xF102;
        const byte Root = 255;
        const float FlushInterval = 0.2f;
        const float ReachDistance = 6f;
        const float CursorDistance = 2.5f;

        enum Kind : byte {
            FloatDelta = 1,
            IntDelta = 2,
            BoolSet = 3,
            AddComponent = 4,
            RemoveComponent = 5,
            FloatSet = 6,
            IntSet = 7
        }

        sealed class Snapshot {
            public readonly Dictionary<(byte part, Type type), Dictionary<FieldInfo, object>> Parts = new Dictionary<(byte, Type), Dictionary<FieldInfo, object>>();
        }

        struct Change {
            public Kind Kind;
            public float F;
            public int I;
            public bool B;
        }

        static readonly Dictionary<Type, FieldInfo[]> fieldCache = new Dictionary<Type, FieldInfo[]>();
        static readonly Dictionary<(byte part, string type, string field), Change> pending = new Dictionary<(byte, string, string), Change>();
        static readonly Assembly gameAssembly = typeof(Body).Assembly;
        static readonly List<(byte part, Type type)> addedOnAvatar = new List<(byte, Type)>();

        static ModLoader.Logger log;
        static Func<bool> session;
        static int depth;
        static Snapshot before;
        static Body target;
        static ushort targetId;
        static float nextFlush;
        static bool external;

        public static bool Handles(ushort method) => method == OpTreat;

        public static void Install(Harmony harmony, ModLoader.Logger logger, Func<bool> inSession) {
            log = logger;
            session = inSession;
            var pre = new HarmonyMethod(typeof(Medical), nameof(ScopePrefix)) { priority = Priority.First };
            var post = new HarmonyMethod(typeof(Medical), nameof(ScopePostfix)) { priority = Priority.Last };
            foreach (var m in new MethodBase[] {
                AccessTools.DeclaredMethod(typeof(PlayerCamera), nameof(PlayerCamera.ApplyWoundItem)),
                AccessTools.DeclaredMethod(typeof(PlayerCamera), nameof(PlayerCamera.WoundSpecialAction)),
                AccessTools.DeclaredMethod(typeof(MinigameBase), "Update")
            }) {
                if (m != null) {
                    harmony.Patch(m, prefix: pre, postfix: post);
                }
            }
            harmony.Patch(AccessTools.DeclaredMethod(typeof(PlayerCamera), nameof(PlayerCamera.ToggleWoundView)),
                prefix: new HarmonyMethod(typeof(Medical), nameof(TogglePrefix)),
                postfix: new HarmonyMethod(typeof(Medical), nameof(TogglePostfix)));
            harmony.Patch(AccessTools.DeclaredMethod(typeof(MinigameBase), nameof(MinigameBase.EndMinigame)),
                postfix: new HarmonyMethod(typeof(Medical), nameof(EndMinigamePostfix)));
            log.Info("medical sync ready");
        }

        static Body Local => PlayerCamera.main != null ? PlayerCamera.main.body : null;

        public static Body Treating {
            get {
                var v = WoundView.view;
                return v != null && v.body != null && RemoteGate.IsRemote(v.body) ? v.body : null;
            }
        }

        static void TogglePrefix(PlayerCamera __instance) {
            if (!session() || __instance.woundView == null || __instance.woundView.activeSelf || WoundView.view == null) {
                return;
            }
            WoundView.view.body = PickTarget() ?? __instance.body;
        }

        static void TogglePostfix(PlayerCamera __instance) {
            if (__instance.woundView != null && !__instance.woundView.activeSelf && WoundView.view != null && WoundView.view.body != __instance.body) {
                Flush();
                WoundView.view.body = __instance.body;
            }
        }

        static Body PickTarget() {
            var local = Local;
            var cam = Camera.main;
            if (local == null || cam == null) {
                return null;
            }
            Vector2 cursor = cam.ScreenToWorldPoint(Input.mousePosition);
            var list = new List<(ushort, Body)>();
            Avatars.Collect(list);
            Body best = null;
            var bestD = CursorDistance;
            foreach (var (_, b) in list) {
                if (b == null || Vector2.Distance(b.transform.position, local.transform.position) > ReachDistance) {
                    continue;
                }
                foreach (var l in b.limbs) {
                    var d = Vector2.Distance(l.transform.position, cursor);
                    if (d < bestD) {
                        bestD = d;
                        best = b;
                    }
                }
            }
            return best;
        }

        public static bool OpenOn(Body body) {
            var cam = PlayerCamera.main;
            if (cam == null || WoundView.view == null || body == null) {
                return false;
            }
            WoundView.view.body = body;
            cam.selectedLimb = body.limbs[Mathf.Clamp(WoundView.view.limbLookingAt, 0, body.limbs.Length - 1)];
            return true;
        }

        public static void BeginExternal(Body patient) {
            if (WoundView.view == null || patient == null) {
                return;
            }
            external = true;
            WoundView.view.body = patient;
        }

        static void EndMinigamePostfix(MinigameBase __instance) {
            if (!external || __instance.currentMinigame) {
                return;
            }
            external = false;
            var cam = PlayerCamera.main;
            if (cam != null && (cam.woundView == null || !cam.woundView.activeSelf)) {
                Close();
            } else {
                Flush();
            }
        }

        public static void Close() {
            Flush();
            if (WoundView.view != null && Local != null) {
                WoundView.view.body = Local;
                PlayerCamera.main.selectedLimb = null;
            }
        }

        static void ScopePrefix(out bool __state) {
            __state = false;
            var t = Treating;
            if (t == null || !session()) {
                return;
            }
            __state = true;
            if (depth++ > 0) {
                return;
            }
            target = t;
            var id = Avatars.IdOf(t);
            targetId = (ushort)Math.Max(0, id);
            before = id < 0 ? null : Capture(t);
        }

        static void ScopePostfix(bool __state) {
            if (!__state || --depth > 0) {
                return;
            }
            if (before != null && target != null) {
                Accumulate(before, Capture(target));
                foreach (var (part, type) in addedOnAvatar) {
                    var go = part == Root ? target.gameObject : (part < target.limbs.Length ? target.limbs[part].gameObject : null);
                    var c = go != null ? go.GetComponent(type) : null;
                    if (c != null) {
                        UnityEngine.Object.DestroyImmediate(c);
                    }
                }
                addedOnAvatar.Clear();
            }
            before = null;
            if (Time.time >= nextFlush) {
                Flush();
            }
        }

        static FieldInfo[] FieldsOf(Type t) {
            if (fieldCache.TryGetValue(t, out var f)) {
                return f;
            }
            var list = new List<FieldInfo>();
            foreach (var fi in t.GetFields(BindingFlags.Public | BindingFlags.Instance)) {
                if (fi.FieldType == typeof(float) || fi.FieldType == typeof(int) || fi.FieldType == typeof(bool)) {
                    list.Add(fi);
                }
            }
            f = list.ToArray();
            fieldCache[t] = f;
            return f;
        }

        static Snapshot Capture(Body b) {
            var s = new Snapshot();
            CapturePart(s, Root, b.gameObject);
            for (var i = 0; i < b.limbs.Length && i < Root; i++) {
                CapturePart(s, (byte)i, b.limbs[i].gameObject);
            }
            return s;
        }

        static void CapturePart(Snapshot s, byte part, GameObject go) {
            foreach (var c in go.GetComponents<MonoBehaviour>()) {
                if (c == null) {
                    continue;
                }
                var t = c.GetType();
                if (t.Assembly != gameAssembly) {
                    continue;
                }
                var values = new Dictionary<FieldInfo, object>();
                foreach (var fi in FieldsOf(t)) {
                    values[fi] = fi.GetValue(c);
                }
                s.Parts[(part, t)] = values;
            }
        }

        static void Accumulate(Snapshot a, Snapshot b) {
            foreach (var kv in b.Parts) {
                var (part, type) = kv.Key;
                if (!a.Parts.TryGetValue(kv.Key, out var old)) {
                    pending[(part, type.FullName, "")] = new Change { Kind = Kind.AddComponent };
                    addedOnAvatar.Add((part, type));
                    foreach (var fv in kv.Value) {
                        var k = (part, type.FullName, fv.Key.Name);
                        if (fv.Value is float af) pending[k] = new Change { Kind = Kind.FloatSet, F = af };
                        else if (fv.Value is int ai) pending[k] = new Change { Kind = Kind.IntSet, I = ai };
                        else if (fv.Value is bool ab) pending[k] = new Change { Kind = Kind.BoolSet, B = ab };
                    }
                    continue;
                }
                foreach (var fv in kv.Value) {
                    old.TryGetValue(fv.Key, out var prev);
                    Set(part, type, fv.Key, fv.Value, prev);
                }
            }
            foreach (var key in a.Parts.Keys) {
                if (!b.Parts.ContainsKey(key)) {
                    pending[(key.part, key.type.FullName, "")] = new Change { Kind = Kind.RemoveComponent };
                }
            }
        }

        static void Set(byte part, Type type, FieldInfo fi, object now, object prev) {
            var key = (part, type.FullName, fi.Name);
            pending.TryGetValue(key, out var ch);
            if (ch.Kind == Kind.FloatSet || ch.Kind == Kind.IntSet) {
                if (now is float sf) ch.F = sf;
                else if (now is int si) ch.I = si;
                pending[key] = ch;
                return;
            }
            if (now is float f) {
                var d = f - (prev is float p ? p : 0f);
                if (Mathf.Abs(d) < 1e-5f) return;
                ch.Kind = Kind.FloatDelta;
                ch.F += d;
            } else if (now is int i) {
                var d = i - (prev is int p ? p : 0);
                if (d == 0) return;
                ch.Kind = Kind.IntDelta;
                ch.I += d;
            } else if (now is bool bo) {
                if (prev is bool p && p == bo) return;
                ch.Kind = Kind.BoolSet;
                ch.B = bo;
            } else {
                return;
            }
            pending[key] = ch;
        }

        public static void Update() {
            if (pending.Count > 0 && Time.time >= nextFlush && depth == 0) {
                Flush();
            }
        }

        static void Flush() {
            nextFlush = Time.time + FlushInterval;
            if (pending.Count == 0 || target == null) {
                pending.Clear();
                return;
            }
            using (var ms = new MemoryStream()) {
                using (var w = new BinaryWriter(ms)) {
                    w.Write(targetId);
                    w.Write((ushort)pending.Count);
                    foreach (var kv in pending) {
                        w.Write(kv.Key.part);
                        w.Write(kv.Key.type);
                        w.Write(kv.Key.field);
                        w.Write((byte)kv.Value.Kind);
                        switch (kv.Value.Kind) {
                            case Kind.FloatDelta:
                            case Kind.FloatSet: w.Write(kv.Value.F); break;
                            case Kind.IntDelta:
                            case Kind.IntSet: w.Write(kv.Value.I); break;
                            case Kind.BoolSet: w.Write(kv.Value.B); break;
                        }
                    }
                }
                Ct.EntityCall(0, OpTreat, 0, 0, ms.ToArray());
                if (Together.Verbose) {
                    log.Debug($"forwarded {pending.Count} treatment changes to player {targetId}");
                }
            }
            pending.Clear();
        }

        public static void Apply(byte[] buf, int offset, int len) {
            using (var r = new BinaryReader(new MemoryStream(buf, offset, len))) {
                try {
                    var target = r.ReadUInt16();
                    if (target != Ct.ct_local_id() || Local == null) {
                        return;
                    }
                    var b = Local;
                    var n = r.ReadUInt16();
                    var applied = new List<string>();
                    var entries = new List<(byte part, string type, string field, Kind kind, float f, int i, bool b)>();
                    for (var i = 0; i < n; i++) {
                        var part = r.ReadByte();
                        var typeName = r.ReadString();
                        var field = r.ReadString();
                        var kind = (Kind)r.ReadByte();
                        float f = 0f;
                        int iv = 0;
                        var bv = false;
                        if (kind == Kind.FloatDelta || kind == Kind.FloatSet) f = r.ReadSingle();
                        else if (kind == Kind.IntDelta || kind == Kind.IntSet) iv = r.ReadInt32();
                        else if (kind == Kind.BoolSet) bv = r.ReadBoolean();
                        entries.Add((part, typeName, field, kind, f, iv, bv));
                    }
                    entries.Sort((x, y) => (x.kind == Kind.AddComponent || x.kind == Kind.RemoveComponent ? 0 : 1).CompareTo(y.kind == Kind.AddComponent || y.kind == Kind.RemoveComponent ? 0 : 1));
                    foreach (var (part, typeName, field, kind, f, iv, bv) in entries) {
                        var go = part == Root ? b.gameObject : (part < b.limbs.Length ? b.limbs[part].gameObject : null);
                        var type = gameAssembly.GetType(typeName);
                        if (go == null || type == null || !typeof(MonoBehaviour).IsAssignableFrom(type)) {
                            continue;
                        }
                        var comp = go.GetComponent(type);
                        if (kind == Kind.AddComponent) {
                            if (comp == null) {
                                go.AddComponent(type);
                                applied.Add("+" + type.Name + (part == Root ? "" : " on limb " + part));
                            }
                            continue;
                        }
                        if (kind == Kind.RemoveComponent) {
                            if (comp != null) {
                                UnityEngine.Object.Destroy(comp);
                                applied.Add("-" + type.Name + (part == Root ? "" : " on limb " + part));
                            }
                            continue;
                        }
                        if (comp == null) {
                            continue;
                        }
                        var fi = type.GetField(field, BindingFlags.Public | BindingFlags.Instance);
                        if (fi == null) {
                            continue;
                        }
                        if (kind == Kind.FloatDelta && fi.FieldType == typeof(float)) fi.SetValue(comp, (float)fi.GetValue(comp) + f);
                        else if (kind == Kind.FloatSet && fi.FieldType == typeof(float)) fi.SetValue(comp, f);
                        else if (kind == Kind.IntDelta && fi.FieldType == typeof(int)) fi.SetValue(comp, (int)fi.GetValue(comp) + iv);
                        else if (kind == Kind.IntSet && fi.FieldType == typeof(int)) fi.SetValue(comp, iv);
                        else if (kind == Kind.BoolSet && fi.FieldType == typeof(bool)) fi.SetValue(comp, bv);
                    }
                    ShowSplintSprites(b);
                    if (Together.Verbose) {
                        var names = new List<string>();
                        foreach (var e in entries) {
                            if (e.part == Root && (e.kind == Kind.FloatDelta || e.kind == Kind.FloatSet)) {
                                names.Add($"{e.field}{(e.kind == Kind.FloatDelta ? (e.f >= 0 ? "+" : "") : "=")}{e.f:0.##}");
                            }
                        }
                        log.Debug($"body: {string.Join(", ", names)} -> heart {b.heartRate:0}, pressure {b.bloodPressure:0}, oxygen {b.bloodOxygen:0}");
                    }
                    log.Info($"treated by another player: {n} changes" + (applied.Count > 0 ? " (" + string.Join(", ", applied) + ")" : ""));
                } catch (EndOfStreamException) {
                }
            }
        }

        static void ShowSplintSprites(Body b) {
            foreach (var l in b.limbs) {
                var s = l.GetComponent<SplintLimb>();
                var marker = l.GetComponent<SplintVisualMarker>();
                if (s == null && marker != null) {
                    UnityEngine.Object.Destroy(marker);
                    continue;
                }
                if (s == null || string.IsNullOrEmpty(s.item) || l.GetComponent<SplintVisualMarker>() != null) {
                    continue;
                }
                var prefab = Resources.Load<GameObject>(s.item);
                var sprite = prefab != null ? prefab.GetComponent<SpriteRenderer>()?.sprite : null;
                if (sprite != null) {
                    l.CreateTemporarySprite(sprite, -90f, null, false, 1000f, x => !x.GetComponent<SplintLimb>());
                }
                l.gameObject.AddComponent<SplintVisualMarker>();
            }
        }
    }

    sealed class SplintVisualMarker : MonoBehaviour {
    }
}

using System;
using System.Collections.Generic;
using System.IO;
using HarmonyLib;
using UnityEngine;

namespace CasualtiesTogether {
    static class Health {
        public const byte MotionKind = 2;
        public const ushort OpDamage = 0xF100;
        const float SendInterval = 0.25f;
        const int MaxLimbs = 24;

        const byte FDismembered = 1;
        const byte FBroken = 2;
        const byte FDislocated = 4;
        const byte FSplinted = 8;
        const byte FInfected = 16;

        struct LimbState {
            public float Skin;
            public float Muscle;
            public float Bleed;
            public float Pain;
            public float Infection;
            public int Shrapnel;
            public byte Flags;
        }

        sealed class BodyState {
            public float Brain;
            public float Consciousness;
            public float Blood;
            public float Shock;
            public float Heart;
            public float Oxygen;
            public float Pressure;
            public float Fibrillation;
            public int LimbCount;
            public readonly LimbState[] Limbs = new LimbState[MaxLimbs];
            public float Received;
        }

        static readonly Dictionary<ushort, BodyState> remote = new Dictionary<ushort, BodyState>();
        static readonly Dictionary<Body, BodyState> before = new Dictionary<Body, BodyState>();
        static readonly MemoryStream sendStream = new MemoryStream(512);
        static readonly List<(ushort id, Body body)> avatars = new List<(ushort, Body)>();

        static ModLoader.Logger log;
        static Func<bool> session;
        static float nextSend;
        static int scopeDepth;
        public static bool Applying;

        public static void Install(Harmony harmony, ModLoader.Logger logger, Func<bool> inSession) {
            log = logger;
            session = inSession;
            var pre = new HarmonyMethod(typeof(Health), nameof(ScopePrefix)) { priority = Priority.First };
            var post = new HarmonyMethod(typeof(Health), nameof(ScopePostfix)) { priority = Priority.Last };
            var fin = new HarmonyMethod(typeof(Health), nameof(ScopeFinalizer));
            foreach (var m in new[] {
                AccessTools.DeclaredMethod(typeof(Body), nameof(Body.Attack)),
                AccessTools.DeclaredMethod(typeof(GunScript), nameof(GunScript.Fire)),
                AccessTools.DeclaredMethod(typeof(CustomItemBehaviour), nameof(CustomItemBehaviour.DynamiteExplode))
            }) {
                if (m == null) {
                    log.Warning("health sync: damage source method missing");
                    continue;
                }
                harmony.Patch(m, prefix: pre, postfix: post, finalizer: fin);
            }
            var filter = new HarmonyMethod(typeof(Health), nameof(CollisionFilter)) { priority = Priority.First };
            var filtered = 0;
            foreach (var t in new[] { typeof(MineScript), typeof(SawbladeScript), typeof(DamagingCrate) }) {
                foreach (var name in new[] { "OnCollisionEnter2D", "OnCollisionStay2D" }) {
                    var m = AccessTools.DeclaredMethod(t, name);
                    if (m != null) {
                        harmony.Patch(m, prefix: filter);
                        filtered++;
                    }
                }
            }
            log.Info($"health sync ready, {filtered} hazard collisions ignore avatars");
        }

        static bool CollisionFilter(Collision2D collision) {
            if (collision == null || collision.collider == null) {
                return true;
            }
            var b = collision.collider.GetComponentInParent<Body>();
            return b == null || !RemoteGate.IsRemote(b);
        }

        static BodyState Capture(Body b, BodyState into = null) {
            var s = into ?? new BodyState();
            s.Brain = b.brainHealth;
            s.Consciousness = b.consciousness;
            s.Blood = b.bloodVolume;
            s.Shock = b.shock;
            s.Heart = b.heartRate;
            s.Oxygen = b.bloodOxygen;
            s.Pressure = b.bloodPressure;
            s.Fibrillation = b.fibrillationProgress;
            s.LimbCount = Mathf.Min(b.limbs.Length, MaxLimbs);
            for (var i = 0; i < s.LimbCount; i++) {
                var l = b.limbs[i];
                byte f = 0;
                if (l.dismembered) f |= FDismembered;
                if (l.broken) f |= FBroken;
                if (l.dislocated) f |= FDislocated;
                if (l.splinted) f |= FSplinted;
                if (l.infected) f |= FInfected;
                s.Limbs[i] = new LimbState {
                    Skin = l.skinHealth, Muscle = l.muscleHealth, Bleed = l.bleedAmount, Pain = l.pain,
                    Infection = l.infectionAmount, Shrapnel = l.shrapnel, Flags = f
                };
            }
            return s;
        }

        public static void Update() {
            if (!session() || !Avatars.InWorld || Time.time < nextSend) {
                return;
            }
            nextSend = Time.time + SendInterval;
            var b = PlayerCamera.main.body;
            var pid = Ct.ct_local_id();
            if (pid < 0) {
                return;
            }
            var s = Capture(b);
            sendStream.SetLength(0);
            var w = new BinaryWriter(sendStream);
            w.Write(MotionKind);
            w.Write((ushort)pid);
            w.Write(s.Brain);
            w.Write(s.Consciousness);
            w.Write(s.Blood);
            w.Write(s.Shock);
            w.Write(s.Heart);
            w.Write(s.Oxygen);
            w.Write(s.Pressure);
            w.Write(s.Fibrillation);
            w.Write((byte)s.LimbCount);
            for (var i = 0; i < s.LimbCount; i++) {
                var l = s.Limbs[i];
                w.Write(l.Flags);
                w.Write((byte)Mathf.Clamp(Mathf.RoundToInt(l.Skin * 2.5f), 0, 255));
                w.Write((byte)Mathf.Clamp(Mathf.RoundToInt(l.Muscle * 2.5f), 0, 255));
                w.Write((ushort)Mathf.Clamp(Mathf.RoundToInt(l.Bleed * 100f), 0, 65535));
                w.Write((ushort)Mathf.Clamp(Mathf.RoundToInt(l.Pain * 10f), 0, 65535));
                w.Write((byte)Mathf.Clamp(Mathf.RoundToInt(l.Infection), 0, 255));
                w.Write((byte)Mathf.Clamp(l.Shrapnel, 0, 255));
            }
            w.Flush();
            Ct.MotionSend(sendStream.GetBuffer(), (int)sendStream.Length);
        }

        public static void OnVitals(byte[] buf, int len) {
            using (var r = new BinaryReader(new MemoryStream(buf, 0, len))) {
                try {
                    r.ReadByte();
                    var pid = r.ReadUInt16();
                    if (!remote.TryGetValue(pid, out var s)) {
                        s = new BodyState();
                        remote[pid] = s;
                    }
                    s.Brain = r.ReadSingle();
                    s.Consciousness = r.ReadSingle();
                    s.Blood = r.ReadSingle();
                    s.Shock = r.ReadSingle();
                    s.Heart = r.ReadSingle();
                    s.Oxygen = r.ReadSingle();
                    s.Pressure = r.ReadSingle();
                    s.Fibrillation = r.ReadSingle();
                    s.LimbCount = Mathf.Min(r.ReadByte(), MaxLimbs);
                    for (var i = 0; i < s.LimbCount; i++) {
                        s.Limbs[i] = new LimbState {
                            Flags = r.ReadByte(),
                            Skin = r.ReadByte() / 2.5f,
                            Muscle = r.ReadByte() / 2.5f,
                            Bleed = r.ReadUInt16() / 100f,
                            Pain = r.ReadUInt16() / 10f,
                            Infection = r.ReadByte(),
                            Shrapnel = r.ReadByte()
                        };
                    }
                    s.Received = Time.time;
                } catch (EndOfStreamException) {
                }
            }
        }

        public static bool ApplyTo(ushort pid, Body b, bool standing) {
            if (!remote.TryGetValue(pid, out var s)) {
                return false;
            }
            Applying = true;
            Items.Muted = true;
            try {
                b.brainHealth = s.Brain;
                b.consciousness = s.Consciousness;
                b.bloodVolume = s.Blood;
                b.shock = standing ? Mathf.Min(s.Shock, 5f) : Mathf.Max(s.Shock, 100f);
                b.heartRate = s.Heart;
                b.bloodOxygen = s.Oxygen;
                b.bloodPressure = s.Pressure;
                b.fibrillationProgress = s.Fibrillation;
                var n = Mathf.Min(s.LimbCount, b.limbs.Length);
                for (var i = 0; i < n; i++) {
                    var l = b.limbs[i];
                    var t = s.Limbs[i];
                    l.skinHealth = t.Skin;
                    l.muscleHealth = t.Muscle;
                    l.bleedAmount = t.Bleed;
                    l.pain = t.Pain;
                    l.infectionAmount = t.Infection;
                    l.shrapnel = t.Shrapnel;
                    l.splinted = (t.Flags & FSplinted) != 0;
                    l.infected = (t.Flags & FInfected) != 0;
                    var dis = (t.Flags & FDismembered) != 0;
                    if (dis && !l.dismembered) {
                        if (Together.Verbose) {
                            log.Debug($"avatar {pid}: limb {i} dismembered");
                        }
                        l.Dismember();
                    } else if (!dis && l.dismembered) {
                        l.Reattach();
                    }
                    var broken = (t.Flags & FBroken) != 0;
                    if (broken && !l.broken) {
                        l.BreakBone();
                    } else if (!broken && l.broken) {
                        l.MendBone();
                    }
                    var disl = (t.Flags & FDislocated) != 0;
                    if (disl && !l.dislocated) {
                        l.Dislocate();
                    } else if (!disl && l.dislocated) {
                        l.UnDislocate();
                    }
                }
            } catch (Exception e) {
                log.Warning($"apply vitals for {pid} failed: {e.Message}");
            } finally {
                Items.Muted = false;
                Applying = false;
            }
            return true;
        }

        public static void Forget(ushort pid) {
            remote.Remove(pid);
        }

        static bool PlayerInitiated(object instance) {
            var local = PlayerCamera.main != null ? PlayerCamera.main.body : null;
            switch (instance) {
                case Body b:
                    return b == local;
                default:
                    return true;
            }
        }

        static bool playerScope;

        static void ScopePrefix(object __instance, out bool __state) {
            __state = scopeDepth > 0 || (session() && PlayerInitiated(__instance));
            if (__state) {
                var outer = scopeDepth == 0;
                Begin();
                if (outer) {
                    playerScope = true;
                    Enemies.BeginPlayerScope();
                }
            }
        }

        static void ScopePostfix(bool __state) {
            if (!__state) {
                return;
            }
            if (scopeDepth == 1) {
                Enemies.EndPlayerScope();
            }
            End();
            if (scopeDepth == 0) {
                playerScope = false;
            }
        }

        public static void Begin() {
            if (scopeDepth++ > 0) {
                return;
            }
            before.Clear();
            Avatars.Collect(avatars);
            foreach (var (_, body) in avatars) {
                if (body != null) {
                    before[body] = Capture(body);
                }
            }
        }

        public static void End() {
            if (scopeDepth <= 0 || --scopeDepth > 0 || before.Count == 0) {
                return;
            }
            if (playerScope && !Modifiers.FriendlyFire) {
                before.Clear();
                return;
            }
            foreach (var (id, body) in avatars) {
                if (body == null || !before.TryGetValue(body, out var a)) {
                    continue;
                }
                var bytes = Diff(a, Capture(body));
                if (bytes != null) {
                    Ct.EntityCall(0, OpDamage, 0, 0, Prefix(id, bytes));
                    if (Together.Verbose) {
                        log.Debug($"forwarded damage to player {id} ({bytes.Length} bytes)");
                    }
                }
            }
            before.Clear();
        }

        static Exception ScopeFinalizer(Exception __exception) {
            if (__exception != null && scopeDepth > 0) {
                scopeDepth = 0;
                playerScope = false;
                before.Clear();
            }
            return __exception;
        }

        static byte[] Prefix(ushort id, byte[] body) {
            var r = new byte[body.Length + 2];
            r[0] = (byte)id;
            r[1] = (byte)(id >> 8);
            Buffer.BlockCopy(body, 0, r, 2, body.Length);
            return r;
        }

        static byte[] Diff(BodyState a, BodyState b) {
            using (var ms = new MemoryStream()) {
                using (var w = new BinaryWriter(ms)) {
                    var any = false;
                    var dBrain = Mathf.Min(0f, b.Brain - a.Brain);
                    var dCons = Mathf.Min(0f, b.Consciousness - a.Consciousness);
                    var dBlood = Mathf.Min(0f, b.Blood - a.Blood);
                    var dShock = Mathf.Max(0f, b.Shock - a.Shock);
                    any |= dBrain < 0f || dCons < 0f || dBlood < 0f || dShock > 0f;
                    w.Write(dBrain);
                    w.Write(dCons);
                    w.Write(dBlood);
                    w.Write(dShock);
                    var n = Mathf.Min(a.LimbCount, b.LimbCount);
                    w.Write((byte)n);
                    for (var i = 0; i < n; i++) {
                        var x = a.Limbs[i];
                        var y = b.Limbs[i];
                        var dSkin = Mathf.Min(0f, y.Skin - x.Skin);
                        var dMuscle = Mathf.Min(0f, y.Muscle - x.Muscle);
                        var dBleed = Mathf.Max(0f, y.Bleed - x.Bleed);
                        var dPain = Mathf.Max(0f, y.Pain - x.Pain);
                        var dShrap = Mathf.Max(0, y.Shrapnel - x.Shrapnel);
                        var newFlags = (byte)(y.Flags & ~x.Flags & (FDismembered | FBroken | FDislocated));
                        any |= dSkin < 0f || dMuscle < 0f || dBleed > 0f || dPain > 0f || dShrap > 0 || newFlags != 0;
                        w.Write(dSkin);
                        w.Write(dMuscle);
                        w.Write(dBleed);
                        w.Write(dPain);
                        w.Write((byte)dShrap);
                        w.Write(newFlags);
                    }
                    w.Flush();
                    return any ? ms.ToArray() : null;
                }
            }
        }

        public static void ApplyDamage(byte[] buf, int offset, int len) {
            if (len < 2) {
                return;
            }
            var target = (ushort)(buf[offset] | (buf[offset + 1] << 8));
            if (target != Ct.ct_local_id() || !Avatars.InWorld) {
                return;
            }
            var b = PlayerCamera.main.body;
            using (var r = new BinaryReader(new MemoryStream(buf, offset + 2, len - 2))) {
                try {
                    b.brainHealth += r.ReadSingle();
                    b.consciousness = Mathf.Max(0f, b.consciousness + r.ReadSingle());
                    b.bloodVolume = Mathf.Max(0f, b.bloodVolume + r.ReadSingle());
                    b.shock += r.ReadSingle();
                    var n = Mathf.Min(r.ReadByte(), b.limbs.Length);
                    for (var i = 0; i < n; i++) {
                        var l = b.limbs[i];
                        l.skinHealth = Mathf.Max(0f, l.skinHealth + r.ReadSingle());
                        l.muscleHealth = Mathf.Max(0f, l.muscleHealth + r.ReadSingle());
                        l.bleedAmount += r.ReadSingle();
                        l.pain += r.ReadSingle();
                        l.shrapnel += r.ReadByte();
                        var flags = r.ReadByte();
                        if ((flags & FDismembered) != 0 && !l.dismembered) {
                            l.Dismember();
                        }
                        if ((flags & FBroken) != 0 && !l.broken) {
                            l.BreakBone();
                        }
                        if ((flags & FDislocated) != 0 && !l.dislocated) {
                            l.Dislocate();
                        }
                    }
                    log.Info("took damage from another player's machine");
                } catch (EndOfStreamException) {
                }
            }
            if (b.shock > 20f && b.standing) {
                b.Ragdoll();
            }
        }
    }
}

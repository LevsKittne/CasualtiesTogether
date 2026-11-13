using System;
using System.Collections.Generic;
using UnityEngine;

namespace CasualtiesTogether {
    static class Modifiers {
        const float HitDrop = 1f;
        const float BurnRate = 5f;
        const float BurnLimit = 3f;
        const float BrainDrop = 2f;

        static ModLoader.Logger log;
        static Func<bool> session;
        static readonly List<(ushort, Body)> avatars = new List<(ushort, Body)>();
        static readonly Dictionary<ushort, bool> wasAlive = new Dictionary<ushort, bool>();
        static readonly Dictionary<int, bool> collisionState = new Dictionary<int, bool>();
        static Body tracked;
        static float[] skin = new float[0];
        static float[] muscle = new float[0];
        static int[] shrapnel = new int[0];
        static byte[] flags = new byte[0];
        static float brain;
        static float burn;
        static float grace;
        static Vector2 baseGravity;
        static bool gravityCaptured;
        static float gravityApplied = 1f;
        static float nextCollision;
        static Collider2D[] localColliders;

        public static void Install(ModLoader.Logger logger, Func<bool> inSession) {
            log = logger;
            session = inSession;
        }

        public static bool FriendlyFire => NetRules.Flag("FriendlyFire") || NetRules.Flag("PVP");

        public static void Update() {
            if (!gravityCaptured) {
                gravityCaptured = true;
                baseGravity = Physics2D.gravity;
            }
            if (!session()) {
                Restore();
                return;
            }
            Gravity();
            if (!Avatars.InWorld) {
                tracked = null;
                wasAlive.Clear();
                return;
            }
            var me = PlayerCamera.main.body;
            Avatars.Collect(avatars);
            OneHit(me);
            SoulLink(me);
            Collision(me);
        }

        static void Restore() {
            if (gravityApplied != 1f) {
                gravityApplied = 1f;
                Physics2D.gravity = baseGravity;
            }
            tracked = null;
            wasAlive.Clear();
            collisionState.Clear();
        }

        static void Gravity() {
            var g = NetRules.Get("GravityMultiplier");
            if (float.IsNaN(g) || g <= 0f) {
                g = 1f;
            }
            if (Mathf.Abs(g - gravityApplied) > 0.0001f) {
                gravityApplied = g;
                Physics2D.gravity = baseGravity * g;
                log.Info($"gravity x{g:0.##}");
            }
        }

        static byte Flags(Limb l) {
            byte f = 0;
            if (l.dismembered) f |= 1;
            if (l.broken) f |= 2;
            if (l.dislocated) f |= 4;
            return f;
        }

        static void Snapshot(Body me) {
            var n = me.limbs.Length;
            if (skin.Length != n) {
                skin = new float[n];
                muscle = new float[n];
                shrapnel = new int[n];
                flags = new byte[n];
            }
            for (var i = 0; i < n; i++) {
                var l = me.limbs[i];
                skin[i] = l.skinHealth;
                muscle[i] = l.muscleHealth;
                shrapnel[i] = l.shrapnel;
                flags[i] = Flags(l);
            }
            brain = me.brainHealth;
        }

        static void OneHit(Body me) {
            if (me != tracked || me.limbs.Length != skin.Length) {
                tracked = me;
                burn = 0f;
                grace = Time.time + 3f;
                Snapshot(me);
                return;
            }
            var on = NetRules.Flag("OneHit") && me.alive && Time.time >= grace;
            if (!on) {
                burn = 0f;
                Snapshot(me);
                return;
            }
            var dt = Mathf.Max(Time.deltaTime, 0.0001f);
            var hit = brain - me.brainHealth > BrainDrop;
            string why = hit ? "brain" : null;
            var sustained = 0f;
            for (var i = 0; i < skin.Length && !hit; i++) {
                var l = me.limbs[i];
                var drop = Mathf.Max(0f, skin[i] - l.skinHealth) + Mathf.Max(0f, muscle[i] - l.muscleHealth);
                if (drop >= HitDrop && drop / dt > 30f) {
                    hit = true;
                    why = $"limb {i} lost {drop:F1}";
                } else if ((Flags(l) & ~flags[i]) != 0) {
                    hit = true;
                    why = $"limb {i} injured";
                } else if (l.shrapnel > shrapnel[i]) {
                    hit = true;
                    why = $"limb {i} shrapnel";
                } else if (drop / dt > BurnRate) {
                    sustained += drop;
                }
            }
            burn = sustained > 0f ? burn + sustained : Mathf.Max(0f, burn - dt);
            if (!hit && burn > BurnLimit) {
                hit = true;
                why = "sustained damage";
            }
            if (hit) {
                log.Info($"one hit: {why}");
                me.brainHealth = 0f;
                burn = 0f;
            }
            Snapshot(me);
        }

        static void SoulLink(Body me) {
            var on = NetRules.Flag("SoulLink");
            foreach (var (id, b) in avatars) {
                if (b == null) {
                    continue;
                }
                var alive = b.alive;
                if (on && me.alive && wasAlive.TryGetValue(id, out var before) && before && !alive) {
                    log.Info($"soul link: {Avatars.NameOf(id)} died");
                    me.brainHealth = 0f;
                }
                wasAlive[id] = alive;
            }
        }

        static void Collision(Body me) {
            if (Time.time < nextCollision) {
                return;
            }
            nextCollision = Time.time + 0.5f;
            var collide = NetRules.Flag("Collision");
            var meKey = me.GetInstanceID();
            if (!collisionState.TryGetValue(meKey, out _)) {
                collisionState.Clear();
                collisionState[meKey] = true;
                localColliders = me.GetComponentsInChildren<Collider2D>(true);
            }
            foreach (var (_, b) in avatars) {
                if (b == null) {
                    continue;
                }
                var key = b.GetInstanceID();
                var known = collisionState.TryGetValue(key, out var state);
                if (known && state == collide) {
                    continue;
                }
                if (!known && collide) {
                    collisionState[key] = true;
                    continue;
                }
                collisionState[key] = collide;
                var theirs = b.GetComponentsInChildren<Collider2D>(true);
                foreach (var a in localColliders) {
                    if (a == null) {
                        continue;
                    }
                    foreach (var c in theirs) {
                        if (c != null) {
                            Physics2D.IgnoreCollision(a, c, !collide);
                        }
                    }
                }
            }
        }
    }
}

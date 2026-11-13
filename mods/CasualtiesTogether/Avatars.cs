using System;
using System.Collections.Generic;
using UnityEngine;

namespace CasualtiesTogether {
    sealed class Avatars {
        const int MaxLimbs = 24;
        const float TeleportSqr = 9f;
        const float PosGain = 10f;
        const float LimbGain = 15f;
        const float AngleGain = 10f;

        sealed class Avatar {
            public ushort Id;
            public string Name;
            public uint Character;
            public GameObject Root;
            public Body Body;
            public PlayerState State;
            public readonly LimbPose[] Limbs = new LimbPose[MaxLimbs];
            public bool HasState;
        }

        readonly Dictionary<ushort, Avatar> avatars = new Dictionary<ushort, Avatar>();
        static Avatars instance;

        public static int IdOf(Body body) {
            if (instance == null || body == null) {
                return -1;
            }
            foreach (var a in instance.avatars.Values) {
                if (a.Body == body) {
                    return a.Id;
                }
            }
            return -1;
        }

        public static void Collect(List<(ushort, Body)> into) {
            into.Clear();
            if (instance == null) {
                return;
            }
            foreach (var a in instance.avatars.Values) {
                if (a.Body != null) {
                    into.Add((a.Id, a.Body));
                }
            }
        }

        public static string NameOf(ushort id) {
            return instance != null && instance.avatars.TryGetValue(id, out var a) ? a.Name : "";
        }

        public static Body BodyOf(ushort id) {
            return instance != null && instance.avatars.TryGetValue(id, out var a) ? a.Body : null;
        }
        readonly List<NetPlayer> players = new List<NetPlayer>();
        readonly List<ushort> stale = new List<ushort>();
        readonly LimbPose[] localLimbs = new LimbPose[MaxLimbs];
        readonly ModLoader.Logger log;
        uint playersRev = uint.MaxValue;
        float nextReport;
        bool localSent;
        GUIStyle tagStyle;

        public Avatars(ModLoader.Logger log) {
            this.log = log;
            instance = this;
        }

        public static bool InWorld {
            get {
                var w = WorldGeneration.world;
                var cam = PlayerCamera.main;
                return w != null && !w.generatingWorld && cam != null && cam.body != null;
            }
        }

        public void Update(bool session) {
            if (!session || !InWorld) {
                if (localSent) {
                    Ct.ct_clear_local_state(IntPtr.Zero, IntPtr.Zero, 0);
                    localSent = false;
                }
                DestroyAll();
                return;
            }
            Capture(PlayerCamera.main.body);
            RefreshPlayers();
            foreach (var p in players) {
                if (p.IsLocal) {
                    continue;
                }
                avatars.TryGetValue(p.Id, out var a);
                if (a == null) {
                    a = new Avatar { Id = p.Id };
                    avatars[p.Id] = a;
                }
                a.Name = p.Name;
                a.HasState = Ct.ct_remote_state(p.Id, out a.State, a.Limbs, MaxLimbs) != 0 && a.State.Has(PlayerState.InWorld);
                if (!a.HasState) {
                    Despawn(a);
                    continue;
                }
                if (a.Body == null || a.Character != (a.State.Character & 0xFF)) {
                    Spawn(a);
                }
                ApplyInputs(a);
            }
            stale.Clear();
            foreach (var id in avatars.Keys) {
                if (!players.Exists(p => p.Id == id)) {
                    stale.Add(id);
                }
            }
            foreach (var id in stale) {
                Despawn(avatars[id]);
                avatars.Remove(id);
                Health.Forget(id);
            }
            Report();
        }

        void Report() {
            if (!Together.Verbose || Time.realtimeSinceStartup < nextReport) {
                return;
            }
            nextReport = Time.realtimeSinceStartup + 5f;
            var me = PlayerCamera.main != null ? PlayerCamera.main.body : null;
            if (me != null) {
                log.Debug($"local: scene={UnityEngine.SceneManagement.SceneManager.GetActiveScene().name} biome={WorldGeneration.world.biomeOverride} pos={(Vector2)me.transform.position} standing={me.standing}");
            }
            foreach (var a in avatars.Values) {
                if (a.Body == null) {
                    continue;
                }
                var err = new Vector2(a.State.X, a.State.Y) - (Vector2)a.Body.transform.position;
                log.Debug($"avatar {a.Name}: standing={a.Body.standing} limbs={a.State.LimbCount} limbErr={LimbError(a):0.000} target=({a.State.X:0.00},{a.State.Y:0.00}) err={err.magnitude:0.000} move=({a.State.MoveX:0.0},{a.State.MoveY:0.0})");
            }
        }

        static float LimbError(Avatar a) {
            if (a.Body.standing || a.State.LimbCount == 0) {
                return 0f;
            }
            var n = Mathf.Min((int)a.State.LimbCount, a.Body.limbs.Length);
            var sum = 0f;
            for (var i = 0; i < n; i++) {
                sum += (new Vector2(a.Limbs[i].X, a.Limbs[i].Y) - (Vector2)a.Body.limbs[i].transform.position).magnitude;
            }
            return sum / n;
        }

        public void FixedUpdate() {
            foreach (var a in avatars.Values) {
                if (a.HasState && a.Body != null) {
                    ApplyPhysics(a);
                }
            }
        }

        void RefreshPlayers() {
            var rev = Ct.ct_players_rev();
            if (rev == playersRev) {
                return;
            }
            playersRev = rev;
            Ct.Players(players);
        }

        void Capture(Body b) {
            var s = new PlayerState();
            Vector2 pos = b.transform.position;
            var vel = b.standing || b.limbs.Length < 2 ? b.rb.velocity : b.limbs[1].rb.velocity;
            s.X = pos.x;
            s.Y = pos.y;
            s.Vx = vel.x;
            s.Vy = vel.y;
            s.LookX = b.targetLookPos.x;
            s.LookY = b.targetLookPos.y;
            s.MoveX = b.moveDir.x;
            s.MoveY = b.moveDir.y;
            s.Character = (uint)Mathf.Clamp(PreRunScript.characterSelected, 0, 255) | (uint)Status.Local(b) << 8;
            uint f = PlayerState.InWorld;
            if (b.standing) f |= PlayerState.Standing;
            if (b.crouching) f |= PlayerState.Crouching;
            if (b.alive) f |= PlayerState.Alive;
            if (b.conscious) f |= PlayerState.Conscious;
            if (b.isRight) f |= PlayerState.FacingRight;
            if (b.grounded) f |= PlayerState.Grounded;
            s.Flags = f;
            var n = 0;
            if (!b.standing) {
                n = Mathf.Min(b.limbs.Length, MaxLimbs);
                for (var i = 0; i < n; i++) {
                    var t = b.limbs[i].transform;
                    var p = t.position;
                    localLimbs[i] = new LimbPose { X = p.x, Y = p.y, Rot = t.eulerAngles.z };
                }
            }
            Ct.ct_set_local_state(ref s, localLimbs, n);
            localSent = true;
        }

        static string ResourceFor(uint character) {
            switch (character) {
                case 0: return "Special/Characters/Experiment";
                case 1: return "Special/Characters/Milky";
                default: return "Special/Characters/Dune";
            }
        }

        void Spawn(Avatar a) {
            Despawn(a);
            var pos = new Vector2(a.State.X, a.State.Y);
            GameObject root;
            try {
                root = Utils.Create(ResourceFor(a.State.Character & 0xFF), pos, 0f);
            } catch (Exception e) {
                log.Error($"cannot spawn avatar for {a.Name}: {e.Message}");
                return;
            }
            var body = root != null ? root.GetComponentInChildren<Body>() : null;
            if (body == null) {
                log.Error($"avatar prefab for character {a.State.Character} has no Body");
                if (root != null) {
                    UnityEngine.Object.Destroy(root);
                }
                return;
            }
            RemoteGate.Mark(body);
            root.name = "CT_Player_" + a.Id;
            body.transform.position = pos;
            a.Root = root;
            a.Body = body;
            a.Character = a.State.Character & 0xFF;
            log.Info($"spawned avatar for {a.Name} at {pos}");
        }

        void Despawn(Avatar a) {
            if (a.Body != null) {
                RemoteGate.Unmark(a.Body);
            }
            if (a.Root != null) {
                UnityEngine.Object.Destroy(a.Root);
            }
            a.Root = null;
            a.Body = null;
        }

        void DestroyAll() {
            if (avatars.Count == 0) {
                return;
            }
            foreach (var a in avatars.Values) {
                Despawn(a);
            }
            avatars.Clear();
        }

        static void ApplyInputs(Avatar a) {
            var b = a.Body;
            var s = a.State;
            var standing = s.Has(PlayerState.Standing);
            b.moveDir = s.Has(PlayerState.Conscious) ? new Vector2(s.MoveX, s.MoveY) : Vector2.zero;
            b.targetLookPos = new Vector3(s.LookX, s.LookY, 0f);
            b.crouching = s.Has(PlayerState.Crouching);
            if (!Health.ApplyTo(a.Id, b, standing)) {
                b.shock = standing ? 0f : 100f;
                b.brainHealth = s.Has(PlayerState.Alive) ? 100f : 0f;
                b.consciousness = s.Has(PlayerState.Conscious) ? 100f : 0f;
                b.bloodVolume = 100f;
            }
            b.hunger = 100f;
            b.thirst = 100f;
            b.energy = 100f;
            b.stamina = 100f;
            b.sleeping = false;
            if (standing && !b.standing) {
                b.Stand(true);
            } else if (!standing && b.standing) {
                b.Ragdoll();
            }
        }

        static void ApplyPhysics(Avatar a) {
            var b = a.Body;
            var s = a.State;
            var target = new Vector2(s.X, s.Y);
            var vel = new Vector2(s.Vx, s.Vy);
            if (b.standing) {
                var err = target - b.rb.position;
                if (err.sqrMagnitude > TeleportSqr) {
                    b.rb.position = target;
                    b.transform.position = target;
                    b.rb.velocity = vel;
                } else {
                    b.rb.velocity = vel + err * PosGain;
                }
                return;
            }
            var n = Mathf.Min((int)s.LimbCount, b.limbs.Length);
            for (var i = 0; i < n; i++) {
                var rb = b.limbs[i].rb;
                if (rb == null) {
                    continue;
                }
                var pose = a.Limbs[i];
                var tp = new Vector2(pose.X, pose.Y);
                var err = tp - rb.position;
                if (err.sqrMagnitude > TeleportSqr) {
                    rb.position = tp;
                    rb.rotation = pose.Rot;
                    rb.velocity = vel;
                    continue;
                }
                rb.velocity = vel + err * LimbGain;
                rb.angularVelocity = Mathf.DeltaAngle(rb.rotation, pose.Rot) * AngleGain;
            }
        }

        public void OnGUI() {
            if (avatars.Count == 0) {
                return;
            }
            var names = NetRules.Flag("EnableNametags") && !(NetRules.Flag("UnchippedHideNametags") && Status.Unchipped());
            var icons = NetRules.Flag("EnableStatusIcons");
            var arrows = NetRules.Flag("ShowPlayerDirections");
            if (!names && !icons && !arrows) {
                return;
            }
            var cam = Camera.main;
            if (cam == null) {
                return;
            }
            if (tagStyle == null) {
                tagStyle = new GUIStyle(GUI.skin.label) { alignment = TextAnchor.MiddleCenter, fontStyle = FontStyle.Bold };
                tagStyle.normal.textColor = Color.white;
            }
            foreach (var a in avatars.Values) {
                if (a.Body == null || a.Body.limbs.Length == 0) {
                    continue;
                }
                var head = a.Body.limbs[0].transform.position + Vector3.up * 1.2f;
                var sp = cam.WorldToScreenPoint(head);
                if (sp.z <= 0f) {
                    continue;
                }
                var onScreen = sp.x >= 0f && sp.x <= Screen.width && sp.y >= 0f && sp.y <= Screen.height;
                if (!onScreen) {
                    if (arrows && a.Body.alive) {
                        Status.DrawArrow(sp, a.Name, tagStyle);
                    }
                    continue;
                }
                if (names && a.Body.alive) {
                    GUI.Label(new Rect(sp.x - 120f, Screen.height - sp.y - 12f, 240f, 24f), a.Name, tagStyle);
                }
                if (icons && (a.State.Character >> 8) != 0) {
                    GUI.Label(new Rect(sp.x - 120f, Screen.height - sp.y - 32f, 240f, 24f), Status.Describe((byte)(a.State.Character >> 8)), tagStyle);
                }
            }
        }

        public void Shutdown() {
            DestroyAll();
        }
    }
}

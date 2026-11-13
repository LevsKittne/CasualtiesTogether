using System;
using System.Collections.Generic;
using System.Runtime.InteropServices;
using System.Text;
using HarmonyLib;
using UnityEngine;

namespace CasualtiesTogether {
    static class Status {
        public const byte Typing = 1;
        public const byte Inventory = 2;
        public const byte Crafting = 4;
        public const byte Trading = 8;
        public const byte Treating = 16;
        public const byte Away = 32;
        public const byte Spectating = 64;

        [DllImport("ct.dll")] static extern int ct_local_typing();

        static readonly StringBuilder sb = new StringBuilder();

        public static byte Local(Body b) {
            byte s = 0;
            var pc = PlayerCamera.main;
            if (ct_local_typing() != 0) {
                s |= Typing;
            }
            if (pc != null) {
                if (pc.radialOpen || (pc.containerMenu != null && pc.containerMenu.activeSelf)) {
                    s |= Inventory;
                }
                if (pc.craftingPanel != null && pc.craftingPanel.activeSelf) {
                    s |= Crafting;
                }
                if (pc.tradeMenu != null && pc.tradeMenu.activeSelf) {
                    s |= Trading;
                }
            }
            var wv = WoundView.view;
            if (wv != null && wv.gameObject.activeInHierarchy && wv.body != null && wv.body != b) {
                s |= Treating;
            }
            if (!Application.isFocused) {
                s |= Away;
            }
            if (Spectator.Target != null) {
                s |= Spectating;
            }
            return s;
        }

        public static string Describe(byte s) {
            sb.Length = 0;
            Add(s, Typing, "typing...");
            Add(s, Inventory, "inventory");
            Add(s, Crafting, "crafting");
            Add(s, Trading, "trading");
            Add(s, Treating, "treating");
            Add(s, Spectating, "spectating");
            Add(s, Away, "away");
            return sb.ToString();
        }

        static void Add(byte s, byte bit, string text) {
            if ((s & bit) == 0) {
                return;
            }
            if (sb.Length > 0) {
                sb.Append(" | ");
            }
            sb.Append(text);
        }

        public static bool Unchipped() {
            try {
                return WorldGeneration.world != null && WorldGeneration.unchipped;
            } catch {
                return false;
            }
        }

        public static void DrawArrow(Vector3 sp, string name, GUIStyle style) {
            var c = new Vector2(Screen.width * 0.5f, Screen.height * 0.5f);
            var d = new Vector2(sp.x, sp.y) - c;
            if (d.sqrMagnitude < 1f) {
                return;
            }
            var m = 28f;
            var sx = (c.x - m) / Mathf.Max(Mathf.Abs(d.x), 0.001f);
            var sy = (c.y - m) / Mathf.Max(Mathf.Abs(d.y), 0.001f);
            var p = c + d * Mathf.Min(sx, sy);
            var angle = Mathf.Atan2(d.y, d.x) * Mathf.Rad2Deg;
            var arrow = angle > 135f || angle < -135f ? "<" : angle > 45f ? "^" : angle < -45f ? "v" : ">";
            GUI.Label(new Rect(p.x - 90f, Screen.height - p.y - 12f, 180f, 24f), arrow + " " + name, style);
        }
    }

    static class Spectator {
        public static Body Target;
        static ModLoader.Logger log;
        static Func<bool> session;
        static readonly List<(ushort, Body)> bodies = new List<(ushort, Body)>();
        static ushort targetId;
        static GUIStyle style;
        public static bool Force;

        public static void Install(Harmony harmony, ModLoader.Logger logger, Func<bool> inSession) {
            log = logger;
            session = inSession;
            harmony.Patch(AccessTools.Method(typeof(PlayerCamera), "HandleCameraPosition"), prefix: new HarmonyMethod(typeof(Spectator), nameof(CameraPrefix)));
        }

        static bool Allowed(Body me) {
            if (!session() || me == null) {
                return false;
            }
            if (Force || !me.alive) {
                return true;
            }
            return !me.conscious && NetRules.Flag("SpectateWhileUnconscious");
        }

        static void CameraPrefix(PlayerCamera __instance) {
            if (Target == null) {
                return;
            }
            if (!Allowed(__instance.body) || !Target.alive) {
                Target = null;
                return;
            }
            Vector2 p = Target.transform.position;
            __instance.customLookPos = p;
        }

        public static void Update() {
            var me = PlayerCamera.main != null ? PlayerCamera.main.body : null;
            if (Target != null && !Allowed(me)) {
                Target = null;
            }
        }

        public static void Next() {
            var me = PlayerCamera.main != null ? PlayerCamera.main.body : null;
            if (!Allowed(me)) {
                log.Info("spectate refused: alive");
                ModLoader.GameConsole.Print("you can spectate only while dead" + (NetRules.Flag("SpectateWhileUnconscious") ? " or unconscious" : ""));
                return;
            }
            Avatars.Collect(bodies);
            bodies.RemoveAll(b => b.Item2 == null || !b.Item2.alive);
            if (bodies.Count == 0) {
                Target = null;
                return;
            }
            bodies.Sort((a, b) => a.Item1.CompareTo(b.Item1));
            var i = Target == null ? 0 : bodies.FindIndex(b => b.Item1 == targetId) + 1;
            if (i >= bodies.Count) {
                Target = null;
                log.Info("spectate: back to own body");
                return;
            }
            targetId = bodies[i].Item1;
            Target = bodies[i].Item2;
            log.Info($"spectate: player {targetId}");
        }

        public static void OnGUI(string name) {
            var me = PlayerCamera.main != null ? PlayerCamera.main.body : null;
            if (!Allowed(me)) {
                return;
            }
            if (style == null) {
                style = new GUIStyle(GUI.skin.label) { alignment = TextAnchor.MiddleCenter, fontStyle = FontStyle.Bold, fontSize = 16 };
                style.normal.textColor = Color.white;
            }
            var text = Target != null ? $"Spectating {name}  ({Ct.BindName(5)}: next)" : $"Press {Ct.BindName(5)} to spectate other players";
            GUI.Label(new Rect(0f, Screen.height - 80f, Screen.width, 30f), text, style);
        }

        public static ushort TargetId => Target != null ? targetId : (ushort)0xFFFF;
    }
}

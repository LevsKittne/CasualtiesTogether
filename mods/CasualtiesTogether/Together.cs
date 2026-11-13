using System;
using System.IO;
using System.Linq;
using HarmonyLib;
using ModLoader;
using Steamworks;
using UnityEngine;
using UnityEngine.SceneManagement;

namespace CasualtiesTogether {
    [ModInfo("casualties-together", "Casualties: Together", "0.1.0", Author = "Levs_Kittne")]
    public sealed class Together : Mod {
        bool loaded;
        bool active;
        bool steamReady;
        Callback<SteamNetworkingMessagesSessionRequest_t> sessionRequest;
        float autoStartAt = -1f;
        string autoStart;
        string[] autoExec;
        Avatars avatars;
        bool startRun;
        bool startTutorial;
        internal static bool Verbose;
        internal static ModLoader.Logger Log;
        float autoExecAt = -1f;

        string PlayerName {
            get {
                var n = Opt("player", "name");
                if (n.Length > 0) {
                    return n;
                }
                var s = steamReady ? Ct.SteamName() : "";
                return s.Length > 0 ? s : Environment.UserName;
            }
        }

        public override void OnLoad() {
            var err = Ct.Load(Path.GetDirectoryName(AssemblyPath));
            if (err != null) {
                Logger.Error(err);
                return;
            }
            Logger.Info($"ct.dll protocol v{Ct.ct_version()}");
            loaded = true;
            Log = Logger;
            Application.runInBackground = true;
            Ct.SetDataDir(Application.persistentDataPath);
            Defaults();
            ApplyArgs();
            RegisterCommands();
            RemoteGate.Install(Harmony, Logger);
            WorldSync.Install(Harmony, Logger, () => active && Ct.ct_is_host() == 0, () => active && Ct.ct_is_host() != 0);
            Entities.Install(Harmony, Logger, () => active, () => active && Ct.ct_is_host() != 0);
            Items.Install(Harmony, Logger, () => active);
            Health.Install(Harmony, Logger, () => active);
            Enemies.Install(Harmony, Logger, () => active, () => active && Ct.ct_is_host() != 0);
            Medical.Install(Harmony, Logger, () => active);
            Voice.Install(Harmony, Logger, () => active);
            Spectator.Install(Harmony, Logger, () => active);
            Modifiers.Install(Logger, () => active);
            Ct.ct_overlay_init();
            PushUiDefaults();
            avatars = new Avatars(Logger);
        }

        readonly System.Collections.Generic.Dictionary<string, string> overrides = new System.Collections.Generic.Dictionary<string, string>();

        string Opt(string section, string key) {
            return overrides.TryGetValue(section + "." + key, out var v) ? v : Config.Get(section, key);
        }

        ushort OptPort() {
            return ushort.TryParse(Opt("net", "port"), out var p) && p != 0 ? p : (ushort)7790;
        }

        void Defaults() {
            var added = false;
            void Def(string s, string k, string v) {
                if (Config.Get(s, k, null) == null) {
                    Config.Set(s, k, v);
                    added = true;
                }
            }
            Def("player", "name", "");
            Def("net", "port", "7790");
            Def("net", "password", "");
            Def("net", "admin_password", "");
            Def("net", "max_players", "0");
            Def("net", "steam", "true");
            if (added && !Config.Save()) {
                Logger.Warning("config file is busy, defaults not written");
            }
        }

        void ApplyArgs() {
            var args = Environment.GetCommandLineArgs();
            string Arg(string key) {
                var i = Array.IndexOf(args, key);
                return i >= 0 && i + 1 < args.Length ? args[i + 1] : null;
            }
            var name = Arg("--mp-setname");
            if (name != null) {
                overrides["player.name"] = name;
            }
            var pass = Arg("--mp-setpass");
            if (pass != null) {
                overrides["net.password"] = pass;
            }
            var adminPass = Arg("--mp-adminpass");
            if (adminPass != null) {
                overrides["net.admin_password"] = adminPass;
            }
            startRun = args.Contains("--mp-immidiate-start");
            startTutorial = args.Contains("--mp-setstarttutorial");
            Verbose = args.Contains("--mp-verbose");
            var exec = Arg("--mp-exec");
            if (exec != null) {
                autoExec = exec.Split(new[] { ';' }, StringSplitOptions.RemoveEmptyEntries);
            }
            var ipPort = Arg("--mp-ip-port");
            if (args.Contains("--mp-starthost")) {
                autoStart = "host";
                if (ipPort != null && ipPort.Contains(":")) {
                    overrides["net.port"] = ipPort.Substring(ipPort.LastIndexOf(':') + 1);
                }
            } else if (args.Contains("--mp-startclient")) {
                autoStart = "client:" + (ipPort ?? "127.0.0.1");
            }
        }

        void RegisterCommands() {
            GameConsole.AddCommand("mp_host", "Host a multiplayer session.", a => {
                var port = a.Length > 1 && ushort.TryParse(a[1], out var p) ? p : OptPort();
                var pass = a.Length > 2 ? a[2] : Opt("net", "password");
                Host(port, pass);
            }, ("int port", "UDP port, default from config."), ("string password", "Optional password."));
            GameConsole.AddCommand("mp_connect", "Connect to a host.", a => {
                if (a.Length < 2) {
                    GameConsole.Print("usage: mp_connect <ip[:port] | steam:ID> [password]");
                    return;
                }
                Connect(a[1], a.Length > 2 ? a[2] : Opt("net", "password"));
            }, ("string address", "ip, ip:port or steam:SteamID64."), ("string password", "Optional password."));
            GameConsole.AddCommand("mp_menu", "Open the multiplayer menu (F1).", a => Ct.ct_overlay_show(1));
            GameConsole.AddCommand("mp_disconnect", "Leave the current session.", a => {
                Ct.ct_disconnect();
                active = false;
                WorldSync.OnSessionEnded();
            });
            GameConsole.AddCommand("mp_status", "Show session, players and ping.", a => {
                foreach (var line in Ct.Status().Split('\n')) {
                    GameConsole.Print(line);
                }
                if (steamReady) {
                    GameConsole.Print($"  steam id: {Ct.ct_steam_id()}");
                }
            });
            GameConsole.AddCommand("mp_name", "Set your multiplayer name.", a => {
                if (a.Length > 1) {
                    Config.Set("player", "name", string.Join(" ", a, 1, a.Length - 1));
                    overrides.Remove("player.name");
                    if (!Config.Save()) {
                        Logger.Warning("config is busy, name not saved");
                    }
                }
                GameConsole.Print("name: " + PlayerName);
            }, ("string name", "New name."));
            GameConsole.AddCommand("mp", "Run a multiplayer command (mp help).", a => {
                Ct.Command(a.Length > 1 ? string.Join(" ", a, 1, a.Length - 1) : "help");
            }, ("string command", "help, players, rules, rule, kick, ban, unban, banlist, admin, login..."));
            GameConsole.AddCommand("mp_entities", "Show synced world entities.", a => {
                GameConsole.Print($"{Entities.Count} entities, layout {Entities.LayoutHash():X8}");
                if (a.Length > 1 && uint.TryParse(a[1], out var id)) {
                    GameConsole.Print(Entities.Describe(id));
                }
            }, ("int id", "Optional entity id to describe."));
            GameConsole.AddCommand("mp_entity_tp", "Teleport to a synced world entity (debug).", a => {
                if (a.Length < 2) {
                    return;
                }
                var id = uint.TryParse(a[1], out var n) ? n : Entities.FirstOfType(a[1]);
                var pos = Entities.PositionOf(id);
                var body = PlayerCamera.main != null ? PlayerCamera.main.body : null;
                if (pos == null || body == null) {
                    GameConsole.Print("no such entity");
                    return;
                }
                body.transform.position = pos.Value + Vector2.up * 1.5f;
                body.rb.velocity = Vector2.zero;
                PlayerCamera.main.transform.position = body.transform.position;
                Logger.Info("debug teleport to " + Entities.Describe(id));
            }, ("string target", "Entity id or type name."));
            GameConsole.AddCommand("mp_items", "Show synced item count (debug).", a => GameConsole.Print($"{Items.Count} items tracked"));
            GameConsole.AddCommand("mp_pick", "Pick the nearest free item into the hand (debug).", a => {
                var body = PlayerCamera.main != null ? PlayerCamera.main.body : null;
                if (body == null) {
                    return;
                }
                Item best = null;
                var bestD = 6f;
                foreach (var it in UnityEngine.Object.FindObjectsOfType<Item>()) {
                    if (it.transform.parent != null || (a.Length > 1 && it.id != a[1])) {
                        continue;
                    }
                    var d = Vector2.Distance(it.transform.position, body.transform.position);
                    if (d < bestD) {
                        bestD = d;
                        best = it;
                    }
                }
                if (best != null) {
                    body.PickUpItem(best, body.handSlot, true);
                    Logger.Info($"debug pick {best.id} -> slot {body.handSlot}: {body.GetItem(body.handSlot) == best}");
                }
            });
            GameConsole.AddCommand("mp_needs", "Show or set hunger, thirst and energy.", a => {
                var body = PlayerCamera.main != null ? PlayerCamera.main.body : null;
                if (body == null) {
                    return;
                }
                if (a.Length > 3) {
                    var inv = System.Globalization.CultureInfo.InvariantCulture;
                    body.hunger = float.Parse(a[1], inv);
                    body.thirst = float.Parse(a[2], inv);
                    body.energy = float.Parse(a[3], inv);
                }
                var line = $"needs: hunger {body.hunger:F1} thirst {body.thirst:F1} energy {body.energy:F1}";
                GameConsole.Print(line);
                Logger.Info(line);
            }, ("float hunger", "Optional new value."), ("float thirst", "Optional new value."), ("float energy", "Optional new value."));
            GameConsole.AddCommand("mp_spectate", "Spectate the next living player while dead.", a => {
                if (a.Length > 1 && a[1] == "force") {
                    Spectator.Force = !Spectator.Force;
                }
                Spectator.Next();
            });
            GameConsole.AddCommand("mp_status_self", "Log your own status bits (debug).", a => Logger.Info("status: " + Status.Describe(Status.Local(PlayerCamera.main != null ? PlayerCamera.main.body : null))));
            GameConsole.AddCommand("mp_perf", "Measure the multiplayer mod's cost per frame (mp_perf starts, mp_perf again reports).", a => {
                if (Perf.Enabled) {
                    var r = Perf.Report();
                    GameConsole.Print(r);
                    Logger.Info(r);
                }
                Perf.Enabled = false;
                Perf.Pending = true;
            });
            GameConsole.AddCommand("mp_voice", "Show voice chat state.", a => {
                var s = Voice.Stats();
                GameConsole.Print(s);
                Logger.Info(s);
            });
            GameConsole.AddCommand("mp_voice_tone", "Send a test tone instead of the microphone (debug).", a => Voice.Tone(a.Length > 1 && float.TryParse(a[1], System.Globalization.NumberStyles.Float, System.Globalization.CultureInfo.InvariantCulture, out var t) ? t : 3f), ("float seconds", "Duration."));
            GameConsole.AddCommand("mp_mp3_play", "Play a built-in song as if picked in the MP3 player (debug).", x => {
                var clips = Resources.LoadAll<AudioClip>("Sounds/music");
                var i = x.Length > 1 && int.TryParse(x[1], out var v) ? v : 0;
                if (MusicManager.main == null || clips.Length == 0) {
                    return;
                }
                AudioClip clip;
                if (i < 0) {
                    clip = AudioClip.Create("ct_test_song", 44100 * 10, 2, 44100, false);
                    var data = new float[44100 * 10 * 2];
                    for (var k = 0; k < data.Length; k++) {
                        data[k] = Mathf.Sin(k / 2 * 2f * Mathf.PI * 220f / 44100f) * 0.3f;
                    }
                    clip.SetData(data, 0);
                } else {
                    clip = clips[Mathf.Clamp(i, 0, clips.Length - 1)];
                }
                if (clip.loadType == AudioClipLoadType.DecompressOnLoad || clip.LoadAudioData()) {
                    MusicManager.main.PlaySong(clip);
                    Voice.PlayPostfix();
                }
            }, ("int index", "Song index."));
            GameConsole.AddCommand("mp_deposit", "Put the held item into the shared stash.", a => Deposit());
            GameConsole.AddCommand("mp_send_save", "Send your save file to the other players.", a => SendSave());
            GameConsole.AddCommand("mp_saves", "List save files received from other players.", a => {
                var dir = Path.Combine(Application.persistentDataPath, "mp_received");
                var files = Directory.Exists(dir) ? Directory.GetFiles(dir) : new string[0];
                for (var i = 0; i < files.Length; i++) {
                    var line = $"{i}: {Path.GetFileName(files[i])} ({new FileInfo(files[i]).Length} bytes)";
                    GameConsole.Print(line);
                    Logger.Info(line);
                }
                if (files.Length == 0) {
                    GameConsole.Print("no received saves");
                }
            });
            GameConsole.AddCommand("mp_use_save", "Replace your save with a received one (mp_saves lists them).", a => {
                var dir = Path.Combine(Application.persistentDataPath, "mp_received");
                var files = Directory.Exists(dir) ? Directory.GetFiles(dir) : new string[0];
                if (a.Length < 2 || !int.TryParse(a[1], out var i) || i < 0 || i >= files.Length) {
                    GameConsole.Print("usage: mp_use_save <index from mp_saves>");
                    return;
                }
                UseSave(files[i]);
            }, ("int index", "Index from mp_saves."));
            GameConsole.AddCommand("mp_spawn", "Spawn an item on the ground next to you (debug).", a => {
                var body = PlayerCamera.main != null ? PlayerCamera.main.body : null;
                if (body == null || a.Length < 2) {
                    return;
                }
                var go = Utils.Create(a[1], (Vector2)body.transform.position + new Vector2(body.isRight ? 3f : -3f, 1f), 0f);
                Logger.Info($"debug spawn {a[1]}: {go != null}");
            }, ("string id", "Item id."));
            GameConsole.AddCommand("mp_spawn_at", "Spawn an item at a player's body (debug).", a => {
                if (a.Length < 3 || !ushort.TryParse(a[1], out var pid)) {
                    return;
                }
                var target = Entities.BodyOfPlayer(pid);
                if (target == null) {
                    GameConsole.Print("no such player body");
                    return;
                }
                var go = Utils.Create(a[2], (Vector2)target.transform.position + Vector2.up, 0f);
                Logger.Info($"debug spawn {a[2]} at player {pid}: {go != null}");
            }, ("int player", "Player id."), ("string id", "Item id."));
            GameConsole.AddCommand("mp_wear", "Wear the nearest free wearable (debug).", a => {
                var body = PlayerCamera.main != null ? PlayerCamera.main.body : null;
                var it = Nearest(body, x => x.transform.parent == null && !string.IsNullOrEmpty(x.Stats?.wearSlotId));
                if (it != null) {
                    body.WearWearable(it);
                    Logger.Info($"debug wear {it.id}: {body.HasWearable(it)}");
                }
            });
            GameConsole.AddCommand("mp_store", "Put the held item into the nearest container (debug).", a => {
                var body = PlayerCamera.main != null ? PlayerCamera.main.body : null;
                var held = body != null ? body.GetItem(body.handSlot) : null;
                if (held == null) {
                    return;
                }
                Container best = null;
                var bestD = 8f;
                foreach (var c in UnityEngine.Object.FindObjectsOfType<Container>()) {
                    if (c.gameObject == held.gameObject) {
                        continue;
                    }
                    var d = Vector2.Distance(c.transform.position, body.transform.position);
                    if (d < bestD) {
                        bestD = d;
                        best = c;
                    }
                }
                if (best != null) {
                    body.DropItem(held);
                    best.LoadItem(held);
                    Logger.Info($"debug store {held.id} -> {best.name}: {held.transform.parent == best.transform}");
                }
            });
            GameConsole.AddCommand("mp_destroy_held", "Destroy the held item (debug).", a => {
                var body = PlayerCamera.main != null ? PlayerCamera.main.body : null;
                var held = body != null ? body.GetItem(body.handSlot) : null;
                if (held != null) {
                    Logger.Info("debug destroy held " + held.id);
                    UnityEngine.Object.Destroy(held.gameObject);
                }
            });
            GameConsole.AddCommand("mp_item_dump", "Log items owned by other players (debug).", a => {
                var mine = (uint)Math.Max(0, Ct.ct_local_id());
                foreach (var ni in UnityEngine.Object.FindObjectsOfType<NetItem>()) {
                    if ((ni.Id & 0x80000000u) == 0 || ((ni.Id >> 20) & 0x7FF) == mine) {
                        continue;
                    }
                    var it = ni.GetComponent<Item>();
                    var p = ni.transform.parent;
                    Logger.Info($"dump #{ni.Id:X8} {it.id} active={ni.gameObject.activeSelf} parent={(p != null ? p.name : "world")}");
                }
            });
            GameConsole.AddCommand("mp_amputate", "Dismember a limb by index (debug).", a => {
                var body = PlayerCamera.main != null ? PlayerCamera.main.body : null;
                if (body != null && a.Length > 1 && int.TryParse(a[1], out var i) && i >= 0 && i < body.limbs.Length) {
                    body.limbs[i].Dismember();
                    Logger.Info($"debug amputate limb {i} ({body.limbs[i].name})");
                }
            }, ("int index", "Limb index."));
            GameConsole.AddCommand("mp_detonate", "Detonate the nearest dynamite (debug).", a => {
                var body = PlayerCamera.main != null ? PlayerCamera.main.body : null;
                var it = Nearest(body, x => x.GetComponent<CustomItemBehaviour>() != null && x.id == "dynamite", 30f);
                if (it != null) {
                    Logger.Info("debug detonate " + it.id);
                    it.GetComponent<CustomItemBehaviour>().DynamiteExplode();
                }
            });
            GameConsole.AddCommand("mp_cpr", "Start CPR on the nearest player (mp_cpr auto = simulated presses, debug).", a => {
                CprMinigame.AutoPress = a.Length > 1 && a[1] == "auto";
                StartCpr();
            }, ("string mode", "auto for simulated presses."));
            GameConsole.AddCommand("mp_treat", "Apply an item to another player's limb (debug).", a => {
                if (a.Length < 4 || !ushort.TryParse(a[1], out var pid) || !int.TryParse(a[2], out var limb)) {
                    return;
                }
                var target = Avatars.BodyOf(pid);
                var body = PlayerCamera.main != null ? PlayerCamera.main.body : null;
                if (target == null || body == null || limb < 0 || limb >= target.limbs.Length) {
                    GameConsole.Print("no such player/limb");
                    return;
                }
                var go = Utils.Create(a[3], body.transform.position, 0f);
                var item = go != null ? go.GetComponent<Item>() : null;
                if (item == null) {
                    return;
                }
                body.PickUpItem(item, body.handSlot, true);
                Medical.OpenOn(target);
                PlayerCamera.main.selectedLimb = target.limbs[limb];
                PlayerCamera.main.ApplyWoundItem(item);
                Logger.Info($"debug treat player {pid} limb {limb} with {a[3]}");
                Medical.Close();
            }, ("int player", "Player id."), ("int limb", "Limb index."), ("string item", "Item id."));
            GameConsole.AddCommand("mp_throw", "Throw the held item (debug).", a => {
                var body = PlayerCamera.main != null ? PlayerCamera.main.body : null;
                if (body != null) {
                    Logger.Info("debug throw " + (body.GetItem(body.handSlot)?.id ?? "nothing"));
                    body.ThrowItem(1f);
                }
            });
            GameConsole.AddCommand("mp_entity_destroy", "Destroy a synced world entity (debug).", a => {
                if (a.Length > 1 && uint.TryParse(a[1], out var id)) {
                    var desc = Entities.Describe(id);
                    GameConsole.Print(Entities.DestroyLocal(id) ? "destroyed " + desc : desc);
                    Logger.Info("debug destroy " + desc);
                }
            }, ("int id", "Entity id."));
            GameConsole.AddCommand("say", "Send a chat message (start with / for a command).", a => {
                if (a.Length > 1) {
                    Ct.Chat(string.Join(" ", a, 1, a.Length - 1));
                }
            }, ("string text", "Message."));
        }

        void Host(ushort port, string password) {
            var steam = steamReady && Config.GetBool("net", "steam", true);
            active = Ct.Host(port, steam, PlayerName, password, Opt("net", "admin_password"), (int)Config.GetFloat("net", "max_players", 0));
            if (active) {
                WorldSync.OnHostStarted();
            }
        }

        void Connect(string addr, string password) {
            active = Ct.Connect(addr, PlayerName, password);
        }

        int execIndex;

        void RunExec() {
            if (autoExec == null || !active) {
                return;
            }
            if (autoExecAt < 0f) {
                if (Avatars.InWorld) {
                    autoExecAt = Time.realtimeSinceStartup + 3f;
                }
                return;
            }
            while (execIndex < autoExec.Length && Time.realtimeSinceStartup >= autoExecAt) {
                var l = autoExec[execIndex++].Trim();
                if (l.StartsWith("wait ") && float.TryParse(l.Substring(5), System.Globalization.NumberStyles.Float, System.Globalization.CultureInfo.InvariantCulture, out var secs)) {
                    autoExecAt = Time.realtimeSinceStartup + secs;
                } else if (l.StartsWith("!")) {
                    GameConsole.Execute(l.Substring(1));
                } else if (l.StartsWith("say ")) {
                    Ct.Chat(l.Substring(4));
                } else if (l.Length > 0) {
                    Ct.Command(l);
                }
            }
            if (execIndex >= autoExec.Length) {
                autoExec = null;
            }
        }

        void StartCpr() {
            var healer = PlayerCamera.main != null ? PlayerCamera.main.body : null;
            if (healer == null || !healer.conscious || !healer.standing || MinigameBase.main == null || MinigameBase.main.currentMinigame) {
                GameConsole.Print("you can't do CPR right now");
                return;
            }
            var list = new System.Collections.Generic.List<(ushort, Body)>();
            Avatars.Collect(list);
            Body target = null;
            var best = 3f;
            foreach (var (_, b) in list) {
                var d = b != null ? Vector2.Distance(b.limbs[1].transform.position, healer.transform.position) : float.MaxValue;
                if (d < best) {
                    best = d;
                    target = b;
                }
            }
            if (target == null) {
                GameConsole.Print("nobody close enough for CPR");
                return;
            }
            if (!CprMinigame.NeedsCpr(target)) {
                GameConsole.Print("they don't seem to need CPR, starting anyway");
            }
            MinigameBase.main.StartMinigame(new CprMinigame(target), null);
            Logger.Info($"CPR started on player {Avatars.IdOf(target)}");
        }

        static Item Nearest(Body body, Func<Item, bool> filter, float radius = 6f) {
            if (body == null) {
                return null;
            }
            Item best = null;
            var bestD = radius;
            foreach (var it in UnityEngine.Object.FindObjectsOfType<Item>()) {
                if (!filter(it)) {
                    continue;
                }
                var d = Vector2.Distance(it.transform.position, body.transform.position);
                if (d < bestD) {
                    bestD = d;
                    best = it;
                }
            }
            return best;
        }

        void PushUiDefaults() {
            Ct.UiDefault("port", OptPort().ToString());
            Ct.UiDefault("password", Opt("net", "password"));
            Ct.UiDefault("admin_password", Opt("net", "admin_password"));
            Ct.UiDefault("max_players", Config.Get("net", "max_players", "0"));
            Ct.UiDefault("steam", Config.GetBool("net", "steam", true) ? "true" : "false");
            Ct.UiDefault("address", Config.Get("net", "last_address", ""));
            Ct.UiDefault("name", PlayerName);
        }

        void SaveConfig() {
            if (!Config.Save()) {
                Logger.Warning("config file is busy, settings not saved");
            }
        }

        void HandleUiActions() {
            string action;
            while ((action = Ct.NextUiAction()) != null) {
                var p = action.Split('\t');
                switch (p[0]) {
                    case "name" when p.Length > 1:
                        Config.Set("player", "name", p[1].Trim());
                        overrides.Remove("player.name");
                        SaveConfig();
                        break;
                    case "host" when p.Length > 6:
                        Config.Set("net", "port", p[1]);
                        Config.Set("net", "password", p[2]);
                        Config.Set("net", "admin_password", p[3]);
                        Config.Set("net", "max_players", p[4]);
                        Config.Set("net", "steam", p[5]);
                        Config.Set("player", "name", p[6].Trim());
                        overrides.Remove("player.name");
                        overrides.Remove("net.password");
                        overrides.Remove("net.admin_password");
                        overrides.Remove("net.port");
                        SaveConfig();
                        Host(OptPort(), Opt("net", "password"));
                        break;
                    case "connect" when p.Length > 3:
                        Config.Set("net", "last_address", p[1]);
                        Config.Set("player", "name", p[3].Trim());
                        overrides.Remove("player.name");
                        SaveConfig();
                        Connect(p[1], p[2]);
                        break;
                    case "send_save":
                        SendSave();
                        break;
                    case "use_save" when p.Length > 1:
                        UseSave(p[1]);
                        break;
                    case "spectate":
                        Spectator.Next();
                        break;
                    case "cpr":
                        StartCpr();
                        break;
                    case "deposit":
                        Deposit();
                        break;
                    case "grant" when p.Length > 2:
                        Grant(p[1], float.TryParse(p[2], System.Globalization.NumberStyles.Float, System.Globalization.CultureInfo.InvariantCulture, out var c) ? c : 1f);
                        break;
                    case "disconnect":
                        Ct.ct_disconnect();
                        active = false;
                        WorldSync.OnSessionEnded();
                        break;
                }
            }
        }

        float nextNeeds;

        void SyncNeeds() {
            if (!active || !Avatars.InWorld || Time.time < nextNeeds) {
                return;
            }
            nextNeeds = Time.time + 0.5f;
            var b = PlayerCamera.main.body;
            float h = b.hunger, t = b.thirst, e = b.energy;
            if (Ct.ct_needs_sync(ref h, ref t, ref e) != 0) {
                b.hunger = h;
                b.thirst = t;
                b.energy = e;
            }
        }

        void UpdateInputBlock() {
            var console = ConsoleScript.instance;
            var typing = console != null && console.active;
            var es = UnityEngine.EventSystems.EventSystem.current;
            var sel = es != null ? es.currentSelectedGameObject : null;
            if (!typing && sel != null) {
                var field = sel.GetComponent<TMPro.TMP_InputField>();
                typing = field != null && field.isFocused;
            }
            Ct.ct_set_input_blocked(typing ? 1 : 0);
        }

        static string SavePath => Path.Combine(Application.persistentDataPath, "save.sv");

        void SendSave() {
            if (!File.Exists(SavePath)) {
                GameConsole.Print("you have no save to send");
                return;
            }
            Ct.FileSend("save.sv", File.ReadAllBytes(SavePath));
        }

        void UseSave(string path) {
            var full = Path.GetFullPath(path);
            var allowed = Path.GetFullPath(Path.Combine(Application.persistentDataPath, "mp_received"));
            if (!full.StartsWith(allowed, StringComparison.OrdinalIgnoreCase) || !File.Exists(full)) {
                Logger.Warning("refusing to use save outside mp_received: " + path);
                return;
            }
            if (File.Exists(SavePath)) {
                File.Copy(SavePath, SavePath + ".bak", true);
            }
            File.Copy(full, SavePath, true);
            Logger.Info($"save replaced with {Path.GetFileName(full)}, previous save kept as save.sv.bak");
            GameConsole.Print("save replaced, use Continue in the main menu to load it (old save: save.sv.bak)");
        }

        void Deposit() {
            var body = PlayerCamera.main != null ? PlayerCamera.main.body : null;
            var held = body != null ? body.GetItem(body.handSlot) : null;
            if (held == null) {
                GameConsole.Print("hold the item you want to put into the stash");
                return;
            }
            if (!active || !NetRules.Flag("SharedStash")) {
                GameConsole.Print("the shared stash is disabled");
                return;
            }
            Ct.StashDeposit(held.id, held.condition);
            UnityEngine.Object.Destroy(held.gameObject);
        }

        void Grant(string prefab, float condition) {
            var body = PlayerCamera.main != null ? PlayerCamera.main.body : null;
            if (body == null) {
                return;
            }
            var go = Utils.Create(prefab, body.transform.position, 0f);
            var item = go != null ? go.GetComponent<Item>() : null;
            if (item == null) {
                Logger.Warning("cannot create stash item " + prefab);
                return;
            }
            item.SetCondition(condition);
            if (body.GetItem(body.handSlot) == null) {
                body.PickUpItem(item, body.handSlot, true);
            } else {
                body.AutoPickUpItem(item);
            }
        }

        void TryStartRun() {
            var prs = PreRunScript.instance;
            if (!startRun || prs == null || autoStart != null) {
                return;
            }
            startRun = false;
            var warning = GameObject.Find("Canvas/Warning");
            if (warning != null) {
                warning.SetActive(false);
            }
            if (startTutorial) {
                prs.StartTutorial();
            } else if (active && Ct.ct_is_host() == 0) {
                Logger.Info("client: the run starts when the host starts one");
                return;
            } else {
                WorldSync.StartRun(prs, null);
            }
            Logger.Info(startTutorial ? "starting tutorial" : "starting run");
        }

        public override void OnLateUpdate() {
            if (loaded) {
                Items.LateUpdate();
            }
        }

        public override void OnFixedUpdate() {
            var t = Perf.Begin();
            avatars?.FixedUpdate();
            if (loaded) {
                Items.FixedUpdate();
                Enemies.FixedUpdate();
            }
            Perf.End(Perf.Fixed, t);
        }

        public override void OnGUI() {
            var t = Perf.Begin();
            avatars?.OnGUI();
            Spectator.OnGUI(Avatars.NameOf(Spectator.TargetId));
            Perf.End(Perf.Gui, t);
        }

        public override void OnSceneLoaded(Scene scene, LoadSceneMode mode) {
            if (autoStart != null && autoStartAt < 0f) {
                autoStartAt = Time.realtimeSinceStartup + 1f;
            }
        }

        public override void OnUpdate() {
            if (!loaded) {
                return;
            }
            if (!steamTried) {
                TryInitSteam();
            }
            if (autoStart != null && autoStartAt >= 0f && Time.realtimeSinceStartup >= autoStartAt) {
                var cmd = autoStart;
                autoStart = null;
                if (cmd == "host") {
                    Host(OptPort(), Opt("net", "password"));
                } else {
                    Connect(cmd.Substring(7), Opt("net", "password"));
                }
            }
            Perf.Frame();
            var t = Perf.Begin();
            TryStartRun();
            WorldSync.Update(active);
            Perf.End(Perf.World, t);
            t = Perf.Begin();
            Items.Update();
            Perf.End(Perf.Items, t);
            t = Perf.Begin();
            Health.Update();
            Perf.End(Perf.Health, t);
            t = Perf.Begin();
            Enemies.Update();
            Perf.End(Perf.Enemies, t);
            t = Perf.Begin();
            Medical.Update();
            Perf.End(Perf.Medical, t);
            t = Perf.Begin();
            HandleUiActions();
            UpdateInputBlock();
            SyncNeeds();
            RunExec();
            Perf.End(Perf.Ui, t);
            t = Perf.Begin();
            avatars?.Update(active);
            Spectator.Update();
            Modifiers.Update();
            Perf.End(Perf.Avatars, t);
            t = Perf.Begin();
            Voice.Update();
            Perf.End(Perf.Voice, t);
            t = Perf.Begin();
            if (active) {
                active = Ct.ct_tick() != 0;
                if (!active) {
                    WorldSync.OnSessionEnded();
                }
            }
            Perf.End(Perf.Tick, t);
            t = Perf.Begin();
            while (Ct.NextNote(out var kind, out var level, out var text)) {
                switch (kind) {
                    case NoteKind.Chat:
                        Chat.Write(LogLevel.Info, text);
                        GameConsole.Print($"<color=#9fd7ff>{text}</color>");
                        break;
                    case NoteKind.Log:
                        Logger.Write((LogLevel)level, text);
                        if (level >= 1) {
                            GameConsole.Print(level >= 3 ? $"<color=red>[MP] {text}</color>" : level == 2 ? $"<color=yellow>[MP] {text}</color>" : $"[MP] {text}");
                        }
                        break;
                }
            }
            Perf.End(Perf.Notes, t);
        }

        void TryInitSteam() {
            var mgr = SteamManager.instance;
            if (mgr == null) {
                return;
            }
            steamTried = true;
            if (!(bool)steamInitialized.GetValue(mgr)) {
                PushUiDefaults();
                Logger.Warning("Steam is not initialized, only direct UDP is available");
                return;
            }
            steamReady = Ct.ct_steam_init() != 0;
            PushUiDefaults();
            if (steamReady) {
                sessionRequest = Callback<SteamNetworkingMessagesSessionRequest_t>.Create(r => {
                    Ct.ct_steam_accept(r.m_identityRemote.GetSteamID64());
                });
            }
        }

        bool steamTried;
        static readonly System.Reflection.FieldInfo steamInitialized = AccessTools.Field(typeof(SteamManager), "initialized");
        static readonly ModLoader.Logger Chat = new ModLoader.Logger("Chat");

        public override void OnQuit() {
            if (loaded) {
                Entities.OnQuit();
                Items.OnQuit();
                avatars?.Shutdown();
                Voice.Shutdown();
                Ct.ct_disconnect();
            }
        }
    }
}

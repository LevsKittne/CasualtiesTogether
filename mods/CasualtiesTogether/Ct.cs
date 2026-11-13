using System;
using System.Collections.Generic;
using System.IO;
using System.Runtime.InteropServices;
using System.Text;

namespace CasualtiesTogether {
    public enum NoteKind : byte {
        Log = 0,
        Chat = 1,
        Players = 2,
        Rules = 3
    }

    public struct NetPlayer {
        public ushort Id;
        public ushort Ping;
        public bool IsHost;
        public bool IsAdmin;
        public bool IsLocal;
        public string Name;
    }

    [StructLayout(LayoutKind.Sequential)]
    public struct LimbPose {
        public float X;
        public float Y;
        public float Rot;
    }

    [StructLayout(LayoutKind.Sequential)]
    public struct PlayerState {
        public float X;
        public float Y;
        public float Vx;
        public float Vy;
        public float LookX;
        public float LookY;
        public float MoveX;
        public float MoveY;
        public uint Flags;
        public uint Character;
        public uint LimbCount;

        public const uint Standing = 1;
        public const uint Crouching = 2;
        public const uint Alive = 4;
        public const uint Conscious = 8;
        public const uint FacingRight = 16;
        public const uint Grounded = 32;
        public const uint InWorld = 64;
        public const uint Climbing = 128;

        public bool Has(uint f) => (Flags & f) != 0;
    }

    static class Ct {
        const string Lib = "ct";

        [DllImport("kernel32", CharSet = CharSet.Unicode, SetLastError = true)]
        static extern IntPtr LoadLibraryW(string path);

        [DllImport(Lib)] public static extern uint ct_version();
        [DllImport(Lib)] static extern void ct_set_data_dir(byte[] path, int len);
        [DllImport(Lib)] public static extern int ct_steam_init();
        [DllImport(Lib)] public static extern ulong ct_steam_id();
        [DllImport(Lib)] static extern int ct_steam_name(byte[] buf, int cap);
        [DllImport(Lib)] public static extern int ct_steam_accept(ulong id);
        [DllImport(Lib)] static extern int ct_host(ushort port, int steam, byte[] name, int nameLen, byte[] password, int passwordLen, byte[] adminPassword, int adminPasswordLen, int maxPlayers);
        [DllImport(Lib)] static extern int ct_connect(byte[] addr, int addrLen, byte[] name, int nameLen, byte[] password, int passwordLen);
        [DllImport(Lib)] public static extern void ct_disconnect();
        [DllImport(Lib)] public static extern int ct_tick();
        [DllImport(Lib)] static extern void ct_chat(byte[] text, int len);
        [DllImport(Lib)] static extern void ct_command(byte[] text, int len);
        [DllImport(Lib)] static extern int ct_next_note(byte[] buf, int cap);
        [DllImport(Lib)] static extern int ct_status(byte[] buf, int cap);
        [DllImport(Lib)] public static extern uint ct_players_rev();
        [DllImport(Lib)] public static extern uint ct_rules_rev();
        [DllImport(Lib)] static extern int ct_players(byte[] buf, int cap);
        [DllImport(Lib)] static extern int ct_rule_index(byte[] name, int len);
        [DllImport(Lib)] public static extern float ct_rule_value(int idx);
        [DllImport(Lib)] public static extern int ct_is_host();
        [DllImport(Lib)] public static extern void ct_set_local_state(ref PlayerState state, LimbPose[] limbs, int limbCount);
        [DllImport(Lib, EntryPoint = "ct_set_local_state")] public static extern void ct_clear_local_state(IntPtr state, IntPtr limbs, int limbCount);
        [DllImport(Lib)] public static extern int ct_remote_state(ushort id, out PlayerState state, LimbPose[] limbs, int cap);

        [DllImport(Lib)] static extern uint ct_world_begin(byte[] blob, int len);
        [DllImport(Lib)] static extern int ct_world_params(byte[] buf, int cap, out uint generation);
        [DllImport(Lib)] public static extern uint ct_world_rev();
        [DllImport(Lib)] public static extern void ct_world_ready(uint generation);
        [DllImport(Lib)] public static extern void ct_tile_edit(ushort x, ushort y, ushort block);
        [DllImport(Lib)] public static extern int ct_tile_poll(TileEdit[] buf, int cap);

        [DllImport(Lib)] static extern void ct_entity_call(uint entity, ushort method, byte flags, uint seed, byte[] args, int len);
        [DllImport(Lib)] public static extern int ct_entity_poll(byte[] buf, int cap);
        [DllImport(Lib)] public static extern int ct_local_id();
        [DllImport(Lib)] static extern void ct_motion_send(byte[] payload, int len);
        [DllImport(Lib)] public static extern int ct_motion_poll(byte[] buf, int cap);

        public static void MotionSend(byte[] payload, int len) {
            ct_motion_send(payload, len);
        }

        public static void EntityCall(uint entity, ushort method, byte flags, uint seed, byte[] args) {
            ct_entity_call(entity, method, flags, seed, args, args?.Length ?? 0);
        }

        [DllImport(Lib)] public static extern int ct_overlay_init();
        [DllImport(Lib)] public static extern void ct_set_input_blocked(int blocked);
        [DllImport(Lib)] static extern int ct_file_send(byte[] name, int nameLen, byte[] data, int len);

        public static bool FileSend(string name, byte[] data) {
            var n = U(name);
            return ct_file_send(n, n.Length, data, data.Length) == 0;
        }
        [DllImport(Lib)] public static extern int ct_needs_sync(ref float hunger, ref float thirst, ref float energy);
        [DllImport(Lib)] static extern void ct_stash_deposit(byte[] prefab, int len, float condition);

        public static void StashDeposit(string prefab, float condition) {
            var p = U(prefab);
            ct_stash_deposit(p, p.Length, condition);
        }
        [DllImport(Lib)] public static extern void ct_overlay_show(int visible);
        [DllImport(Lib)] static extern int ct_bind_name(int idx, byte[] buf, int cap);

        public static string BindName(int idx) {
            var buf = new byte[64];
            var n = ct_bind_name(idx, buf, buf.Length);
            return n < 0 ? "?" : Encoding.UTF8.GetString(buf, 0, Math.Min(n, buf.Length));
        }
        [DllImport(Lib)] static extern void ct_ui_set_default(byte[] key, int keyLen, byte[] value, int valueLen);
        [DllImport(Lib)] static extern int ct_ui_next_action(byte[] buf, int cap);

        public static void UiDefault(string key, string value) {
            var k = U(key);
            var v = U(value);
            ct_ui_set_default(k, k.Length, v, v.Length);
        }

        public static string NextUiAction() {
            var len = ct_ui_next_action(buf, buf.Length);
            return len > 0 ? Encoding.UTF8.GetString(buf, 0, len) : null;
        }

        static readonly byte[] buf = new byte[65536];

        public static uint WorldBegin(byte[] blob) => ct_world_begin(blob, blob.Length);

        public static byte[] WorldParams(out uint generation) {
            var len = ct_world_params(buf, buf.Length, out generation);
            if (len < 0) {
                var big = new byte[-len];
                len = ct_world_params(big, big.Length, out generation);
                return big;
            }
            var r = new byte[len];
            Buffer.BlockCopy(buf, 0, r, 0, len);
            return r;
        }

        public static string Load(string dir) {
            var path = Path.Combine(dir, "ct.dll");
            if (!File.Exists(path)) {
                return "ct.dll not found in " + dir;
            }
            if (LoadLibraryW(path) == IntPtr.Zero) {
                return "LoadLibrary failed: " + Marshal.GetLastWin32Error();
            }
            return null;
        }

        static byte[] U(string s) => Encoding.UTF8.GetBytes(s ?? "");

        public static void SetDataDir(string dir) {
            var d = U(dir);
            ct_set_data_dir(d, d.Length);
        }

        public static bool Host(ushort port, bool steam, string name, string password, string adminPassword, int maxPlayers) {
            var n = U(name);
            var p = U(password);
            var a = U(adminPassword);
            return ct_host(port, steam ? 1 : 0, n, n.Length, p, p.Length, a, a.Length, maxPlayers) == 0;
        }

        public static bool Connect(string addr, string name, string password) {
            var a = U(addr);
            var n = U(name);
            var p = U(password);
            return ct_connect(a, a.Length, n, n.Length, p, p.Length) == 0;
        }

        public static void Chat(string text) {
            var t = U(text);
            ct_chat(t, t.Length);
        }

        public static void Command(string line) {
            var t = U(line);
            ct_command(t, t.Length);
        }

        public static int RuleIndex(string name) {
            var t = U(name);
            return ct_rule_index(t, t.Length);
        }

        public static string SteamName() {
            var len = ct_steam_name(buf, buf.Length);
            return len > 0 ? Encoding.UTF8.GetString(buf, 0, len) : "";
        }

        public static string Status() {
            var len = ct_status(buf, buf.Length);
            return len > 0 ? Encoding.UTF8.GetString(buf, 0, len) : "";
        }

        public static bool NextNote(out NoteKind kind, out int level, out string text) {
            var len = ct_next_note(buf, buf.Length);
            if (len < 2) {
                kind = NoteKind.Log;
                level = 0;
                text = null;
                return false;
            }
            kind = (NoteKind)buf[0];
            level = buf[1];
            text = Encoding.UTF8.GetString(buf, 2, len - 2);
            return true;
        }

        public static void Players(List<NetPlayer> into) {
            into.Clear();
            var len = ct_players(buf, buf.Length);
            if (len < 2) {
                return;
            }
            var count = BitConverter.ToUInt16(buf, 0);
            var pos = 2;
            for (var i = 0; i < count && pos + 6 <= len; i++) {
                var id = BitConverter.ToUInt16(buf, pos);
                var ping = BitConverter.ToUInt16(buf, pos + 2);
                var flags = buf[pos + 4];
                var nameLen = buf[pos + 5];
                pos += 6;
                into.Add(new NetPlayer {
                    Id = id,
                    Ping = ping,
                    IsHost = (flags & 1) != 0,
                    IsAdmin = (flags & 2) != 0,
                    IsLocal = (flags & 4) != 0,
                    Name = Encoding.UTF8.GetString(buf, pos, Math.Min(nameLen, len - pos))
                });
                pos += nameLen;
            }
        }
    }

    public static class NetRules {
        static uint rev = uint.MaxValue;
        static readonly Dictionary<string, int> index = new Dictionary<string, int>();
        static float[] cache = new float[0];

        static void Refresh() {
            var r = Ct.ct_rules_rev();
            if (r == rev) {
                return;
            }
            rev = r;
            for (var i = 0; i < cache.Length; i++) {
                cache[i] = Ct.ct_rule_value(i);
            }
        }

        public static float Get(string name) {
            if (!index.TryGetValue(name, out var i)) {
                i = Ct.RuleIndex(name);
                index[name] = i;
                if (i >= cache.Length) {
                    Array.Resize(ref cache, i + 1);
                    rev = uint.MaxValue;
                }
            }
            if (i < 0) {
                return float.NaN;
            }
            Refresh();
            return cache[i];
        }

        public static bool Flag(string name) => Get(name) != 0f;
    }
}

using System;
using System.Collections.Generic;
using System.Runtime.InteropServices;
using System.Text;
using HarmonyLib;
using UnityEngine;

namespace CasualtiesTogether {
    static class Voice {
        const string Lib = "ct.dll";
        const byte StreamVoice = 0;
        const byte StreamMusic = 1;
        const byte FlagImpaired = 1;
        const byte FlagDead = 2;
        const byte FlagSleeping = 4;
        const int ClipRate = 48000;

        [DllImport(Lib)] static extern int ct_voice_capture(float[] samples, int n, int rate, byte flags);
        [DllImport(Lib)] static extern int ct_music_capture(float[] samples, int frames, int channels, int rate);
        [DllImport(Lib)] static extern void ct_music_stop();
        [DllImport(Lib)] static extern void ct_music_meta_send(byte[] name, int len, double seconds);
        [DllImport(Lib)] static extern int ct_music_meta(ushort from, byte[] buf, int cap, out double seconds, out float volume);
        [DllImport(Lib)] static extern float ct_voice_read(ushort from, byte stream, float[] buf, int n);
        [DllImport(Lib)] static extern float ct_voice_level(ushort from, byte stream);
        [DllImport(Lib)] static extern float ct_voice_listener(float hearingLoss, int dead);
        [DllImport(Lib)] static extern void ct_voice_devices(byte[] text, int len);
        [DllImport(Lib)] static extern int ct_voice_mic(byte[] buf, int cap);

        sealed class Speaker {
            public ushort Id;
            public Body Body;
            public GameObject Go;
            public AudioSource Voice;
            public AudioSource Music;
            public AudioClip Stream;
            public string Song;
        }

        static ModLoader.Logger log;
        static Func<bool> session;
        static readonly Dictionary<ushort, Speaker> speakers = new Dictionary<ushort, Speaker>();
        static readonly List<(ushort, Body)> bodies = new List<(ushort, Body)>();
        static readonly List<ushort> gone = new List<ushort>();
        static readonly byte[] micBuf = new byte[512];
        static string micWanted;
        static string micActive;
        static AudioClip micClip;
        static int micPos;
        static float[] micSamples = new float[4096];
        static float nextMicCheck;
        static float nextDevices;
        static float range = 183f;

        static AudioSource musicSource;
        static AudioClip musicClip;
        static int musicPos;
        static bool musicReadable;
        static float[] musicSamples = new float[16384];
        static bool musicWarned;
        static float nextMeta;
        static readonly byte[] metaBuf = new byte[512];
        static Dictionary<string, AudioClip> builtin;
        static float toneUntil;
        static double tonePhase;
        static float[] toneBuf = new float[4096];

        public static void Install(Harmony harmony, ModLoader.Logger logger, Func<bool> inSession) {
            log = logger;
            session = inSession;
            harmony.Patch(AccessTools.Method(typeof(MP3Menu), nameof(MP3Menu.Play)), postfix: new HarmonyMethod(typeof(Voice), nameof(PlayPostfix)));
        }

        public static void PlayPostfix() {
            var mm = MusicManager.main;
            if (mm == null) {
                return;
            }
            musicSource = Traverse.Create(mm).Field("source").GetValue<AudioSource>();
            musicClip = musicSource != null ? musicSource.clip : null;
            musicPos = 0;
            musicReadable = musicClip != null && musicClip.loadType == AudioClipLoadType.DecompressOnLoad;
            if (musicClip != null) {
                log.Info($"mp3 sync: {musicClip.name} {musicClip.frequency} Hz x{musicClip.channels}, {(musicReadable ? "streaming to players" : "not readable (" + musicClip.loadType + ")")}");
            }
        }

        public static void Update() {
            if (!session()) {
                StopMic();
                Clear();
                return;
            }
            var me = PlayerCamera.main != null ? PlayerCamera.main.body : null;
            range = ct_voice_listener(me != null ? me.hearingLoss / 100f : 0f, me != null && !me.alive ? 1 : 0);
            UpdateMic(me);
            UpdateMusic();
            UpdateSpeakers();
        }

        static void UpdateMic(Body me) {
            if (Time.unscaledTime >= nextDevices) {
                nextDevices = Time.unscaledTime + 5f;
                var b = Encoding.UTF8.GetBytes(string.Join("\t", Microphone.devices));
                ct_voice_devices(b, b.Length);
            }
            if (Time.unscaledTime >= nextMicCheck) {
                nextMicCheck = Time.unscaledTime + 1f;
                var n = ct_voice_mic(micBuf, micBuf.Length);
                micWanted = n < 0 ? null : Encoding.UTF8.GetString(micBuf, 0, Math.Min(n, micBuf.Length));
                if (micWanted != null && micWanted.Length > 0 && Array.IndexOf(Microphone.devices, micWanted) < 0) {
                    micWanted = "";
                }
                if (micWanted != micActive || (micActive != null && micClip == null)) {
                    StopMic();
                    if (micWanted != null && Microphone.devices.Length > 0) {
                        micActive = micWanted;
                        micClip = Microphone.Start(micActive.Length == 0 ? null : micActive, true, 1, ClipRate);
                        micPos = 0;
                        log.Info($"microphone: {(micActive.Length == 0 ? "default" : micActive)} at {(micClip != null ? micClip.frequency : 0)} Hz");
                    }
                }
            }
            if (Time.unscaledTime < toneUntil) {
                var n = Mathf.Clamp((int)(Time.unscaledDeltaTime * ClipRate), 1, toneBuf.Length);
                for (var i = 0; i < n; i++) {
                    tonePhase += 2.0 * Math.PI * 330.0 / ClipRate;
                    toneBuf[i] = (float)Math.Sin(tonePhase) * 0.4f;
                }
                ct_voice_capture(toneBuf, n, ClipRate, 0x80);
                return;
            }
            if (micClip == null) {
                return;
            }
            var dev = micActive.Length == 0 ? null : micActive;
            var pos = Microphone.GetPosition(dev);
            var total = micClip.samples;
            var count = pos >= micPos ? pos - micPos : total - micPos + pos;
            if (count <= 0) {
                return;
            }
            if (count > total / 2) {
                micPos = pos;
                return;
            }
            if (micSamples.Length < count) {
                micSamples = new float[count * 2];
            }
            var first = Math.Min(count, total - micPos);
            Read(micClip, micPos, first, micSamples, 0);
            if (count > first) {
                Read(micClip, 0, count - first, micSamples, first);
            }
            micPos = (micPos + count) % total;
            byte flags = 0;
            if (me != null) {
                if (me.talker != null && me.talker.impairedSpeech) {
                    flags |= FlagImpaired;
                }
                if (!me.alive) {
                    flags |= FlagDead;
                }
                if (me.sleeping) {
                    flags |= FlagSleeping;
                }
            }
            ct_voice_capture(micSamples, count, micClip.frequency, flags);
        }

        static readonly Dictionary<int, float[]> scratch = new Dictionary<int, float[]>();

        static void Read(AudioClip clip, int offset, int n, float[] into, int at) {
            var len = n * clip.channels;
            if (!scratch.TryGetValue(len, out var tmp)) {
                if (scratch.Count > 32) {
                    scratch.Clear();
                }
                tmp = new float[len];
                scratch[len] = tmp;
            }
            clip.GetData(tmp, offset);
            Array.Copy(tmp, 0, into, at * clip.channels, len);
        }

        static void StopMic() {
            if (micActive != null) {
                Microphone.End(micActive.Length == 0 ? null : micActive);
            }
            micActive = null;
            micClip = null;
        }

        static void UpdateMusic() {
            if (musicClip == null) {
                return;
            }
            if (musicSource == null || musicSource.clip != musicClip || !musicSource.isPlaying) {
                musicClip = null;
                ct_music_stop();
                return;
            }
            if (!musicReadable) {
                if (Time.unscaledTime >= nextMeta) {
                    nextMeta = Time.unscaledTime + 1f;
                    var b = Encoding.UTF8.GetBytes(musicClip.name);
                    ct_music_meta_send(b, b.Length, musicSource.time);
                }
                return;
            }
            var pos = musicSource.timeSamples;
            var lead = musicClip.frequency / 10;
            var target = Math.Min(pos + lead, musicClip.samples);
            if (musicPos > target || target - musicPos > musicClip.frequency) {
                musicPos = pos;
                return;
            }
            var count = target - musicPos;
            if (count < musicClip.frequency / 50) {
                return;
            }
            var len = count * musicClip.channels;
            if (musicSamples.Length < len) {
                musicSamples = new float[len * 2];
            }
            Read(musicClip, musicPos, count, musicSamples, 0);
            musicPos = target;
            if (ct_music_capture(musicSamples, count, musicClip.channels, musicClip.frequency) < 0 && !musicWarned) {
                musicWarned = true;
                log.Warning("mp3 sync capture failed");
            }
        }

        static void UpdateSpeakers() {
            Avatars.Collect(bodies);
            gone.Clear();
            foreach (var s in speakers.Values) {
                var alive = false;
                if (s.Go != null && s.Body != null) {
                    for (var i = 0; i < bodies.Count; i++) {
                        if (bodies[i].Item1 == s.Id && bodies[i].Item2 == s.Body) {
                            alive = true;
                            break;
                        }
                    }
                }
                if (!alive) {
                    gone.Add(s.Id);
                }
            }
            foreach (var id in gone) {
                Drop(id);
            }
            foreach (var (id, body) in bodies) {
                if (!speakers.TryGetValue(id, out var s)) {
                    s = Create(id, body);
                    speakers[id] = s;
                }
                var p = body.transform.position;
                s.Go.transform.position = new Vector3(p.x, p.y, 0f);
                Shape(s.Voice, range);
                Shape(s.Music, range * 0.8f);
                SyncSong(s);
            }
        }

        static void SyncSong(Speaker s) {
            var n = ct_music_meta(s.Id, metaBuf, metaBuf.Length, out var secs, out var vol);
            var song = n < 0 ? null : Encoding.UTF8.GetString(metaBuf, 0, Math.Min(n, metaBuf.Length));
            if (song == null) {
                if (s.Song != null) {
                    s.Song = null;
                    s.Music.Stop();
                    s.Music.clip = s.Stream;
                    s.Music.volume = 1f;
                    s.Music.loop = true;
                    s.Music.Play();
                }
                return;
            }
            if (builtin == null) {
                builtin = new Dictionary<string, AudioClip>();
                foreach (var c in Resources.LoadAll<AudioClip>("Sounds/music")) {
                    builtin[c.name] = c;
                }
            }
            if (!builtin.TryGetValue(song, out var clip)) {
                return;
            }
            s.Music.volume = vol;
            if (s.Song != song) {
                s.Song = song;
                s.Music.Stop();
                s.Music.loop = false;
                s.Music.clip = clip;
                s.Music.time = Mathf.Clamp((float)secs, 0f, clip.length - 0.05f);
                s.Music.Play();
                log.Info($"mp3 sync: player {s.Id} plays {song}");
            } else if (Mathf.Abs(s.Music.time - (float)secs) > 0.6f && secs < clip.length) {
                s.Music.time = (float)secs;
            }
        }

        static void Shape(AudioSource src, float max) {
            max = Mathf.Max(max, 5f);
            if (Mathf.Abs(src.maxDistance - max) > 0.01f) {
                src.maxDistance = max;
                src.minDistance = max * 0.15f;
            }
        }

        static Speaker Create(ushort id, Body body) {
            var go = new GameObject("ct_voice_" + id);
            UnityEngine.Object.DontDestroyOnLoad(go);
            var s = new Speaker { Id = id, Body = body, Go = go };
            s.Voice = MakeSource(go, id, StreamVoice);
            s.Music = MakeSource(go, id, StreamMusic);
            s.Stream = s.Music.clip;
            return s;
        }

        static AudioSource MakeSource(GameObject go, ushort id, byte stream) {
            var src = go.AddComponent<AudioSource>();
            src.clip = AudioClip.Create($"ct_{id}_{stream}", ClipRate, 1, ClipRate, true, data => ct_voice_read(id, stream, data, data.Length));
            src.loop = true;
            src.spatialBlend = 1f;
            src.rolloffMode = AudioRolloffMode.Linear;
            src.dopplerLevel = 0f;
            src.spread = 0f;
            src.priority = 32;
            src.maxDistance = range;
            src.minDistance = range * 0.15f;
            src.Play();
            return src;
        }

        static void Drop(ushort id) {
            if (speakers.TryGetValue(id, out var s)) {
                if (s.Go != null) {
                    UnityEngine.Object.Destroy(s.Go);
                }
                speakers.Remove(id);
            }
        }

        static void Clear() {
            if (speakers.Count == 0) {
                return;
            }
            foreach (var id in new List<ushort>(speakers.Keys)) {
                Drop(id);
            }
        }

        public static void Tone(float seconds) {
            toneUntil = Time.unscaledTime + seconds;
        }

        public static string Stats() {
            var sb = new StringBuilder();
            sb.Append($"voice: mic {(micActive == null ? "off" : micActive.Length == 0 ? "default" : micActive)}, range {range:F0}, mp3 {(musicClip != null ? musicClip.name : "-")}");
            foreach (var s in speakers.Values) {
                sb.Append($"\n  [{s.Id}] voice {ct_voice_level(s.Id, StreamVoice):F2} mp3 {ct_voice_level(s.Id, StreamMusic):F2} song {(s.Song ?? "-")} at {(s.Song != null ? s.Music.time : 0f):F1}s playing {s.Voice.isPlaying} at {s.Go.transform.position}");
            }
            return sb.ToString();
        }

        public static float Level(ushort id) {
            return ct_voice_level(id, StreamVoice);
        }

        public static void Shutdown() {
            StopMic();
            Clear();
        }
    }
}

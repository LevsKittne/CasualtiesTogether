using System;
using System.Collections;
using System.Collections.Generic;
using System.IO;
using System.Reflection;
using System.Runtime.InteropServices;
using HarmonyLib;
using UnityEngine;

namespace CasualtiesTogether {
    [StructLayout(LayoutKind.Sequential)]
    public struct TileEdit {
        public ushort X;
        public ushort Y;
        public ushort Block;
    }

    static class WorldSync {
        const int BlobVersion = 1;


        static readonly Dictionary<Type, FieldInfo> stateFields = new Dictionary<Type, FieldInfo>();
        static readonly FieldInfo blocksField = AccessTools.Field(typeof(WorldGeneration), "worldBlocks");
        static readonly FieldInfo startedLoading = AccessTools.Field(typeof(PreRunScript), "startedLoading");
        static readonly MethodInfo waitLoad = AccessTools.Method(typeof(PreRunScript), "WaitLoad");
        static readonly FieldInfo doingRegen = AccessTools.Field(typeof(WorldGeneration), "doingRegen");
        static readonly TileEdit[] tileBuf = new TileEdit[1024];

        static Type generateWorldType;
        static UnityEngine.Random.State genState;
        static UnityEngine.Random.State outerState;
        static int depth;
        static bool applyingRemote;
        static bool genActive;
        static ushort[,] checkpoint;

        static byte[] lastBlob;
        static uint appliedGen;
        static uint knownRev = uint.MaxValue;
        static uint targetGen;
        static byte[] targetBlob;
        static bool wasGenerating;
        static bool followRequested;
        static bool waitingNoted;
        static float waitingSince;
        static float nextHashReport;

        static ModLoader.Logger log;
        static Func<bool> isClient;
        static Func<bool> isHost;

        public static void Install(Harmony harmony, ModLoader.Logger logger, Func<bool> client, Func<bool> host) {
            log = logger;
            isClient = client;
            isHost = host;
            var prefix = new HarmonyMethod(typeof(WorldSync), nameof(StepPrefix)) { priority = Priority.First };
            var postfix = new HarmonyMethod(typeof(WorldSync), nameof(StepPostfix)) { priority = Priority.Last };
            var finalizer = new HarmonyMethod(typeof(WorldSync), nameof(StepFinalizer));
            var patched = 0;
            foreach (var nested in typeof(WorldGeneration).GetNestedTypes(BindingFlags.NonPublic | BindingFlags.Public)) {
                var state = AccessTools.Field(nested, "<>1__state");
                var moveNext = AccessTools.Method(nested, "MoveNext");
                if (state == null || moveNext == null || !typeof(IEnumerator).IsAssignableFrom(nested)) {
                    continue;
                }
                stateFields[nested] = state;
                if (nested.Name.StartsWith("<GenerateWorld>")) {
                    generateWorldType = nested;
                }
                harmony.Patch(moveNext, prefix: prefix, postfix: postfix, finalizer: finalizer);
                patched++;
            }
            harmony.Patch(AccessTools.Method(typeof(WorldGeneration), nameof(WorldGeneration.SetBlock), new[] { typeof(Vector2Int), typeof(ushort) }),
                postfix: new HarmonyMethod(typeof(WorldSync), nameof(SetBlockPostfix)));
            harmony.Patch(AccessTools.Method(typeof(WorldGeneration), nameof(WorldGeneration.SetBlockNoUpdate), new[] { typeof(Vector2Int), typeof(ushort) }),
                postfix: new HarmonyMethod(typeof(WorldSync), nameof(SetBlockNoUpdatePostfix)));
            log.Info($"world sync: {patched} generation steps isolated, tile hooks ready");
        }

        static bool StepPrefix(object __instance, ref bool __result, out bool __state) {
            __state = false;
            if (__instance.GetType() == generateWorldType && (int)stateFields[generateWorldType].GetValue(__instance) == 0) {
                if (!BeginGeneration()) {
                    __result = true;
                    return false;
                }
                genActive = true;
                Entities.Salt = StateHash(genState);
                if (Together.Verbose) {
                    DumpGround("at generation start");
                }
            }
            if (!genActive) {
                return true;
            }
            __state = true;
            if (depth++ == 0) {
                outerState = UnityEngine.Random.state;
                UnityEngine.Random.state = genState;
            }
            return true;
        }

        static void StepPostfix(object __instance, bool __result, bool __state) {
            if (!__state) {
                return;
            }
            Leave();
            if (__result) {
                return;
            }
            if (Together.Verbose && depth == 0 && WorldGeneration.world != null) {
                log.Debug($"gen step {__instance.GetType().Name} done: rng {StateHash(genState):X8} blocks {Hash(WorldGeneration.world):X16}");
            }
            if (__instance.GetType().Name.StartsWith("<WorldPlaceEntities>") && WorldGeneration.world != null) {
                checkpoint = (blocksField.GetValue(WorldGeneration.world) as ushort[,])?.Clone() as ushort[,];
                Entities.Register();
                Items.Register();
            }
            if (__instance.GetType() == generateWorldType) {
                genActive = false;
            }
        }

        static Exception StepFinalizer(Exception __exception, bool __state) {
            if (__exception != null && __state && depth > 0) {
                Leave();
                genActive = false;
            }
            return __exception;
        }

        static void DumpGround(string when) {
            var hits = Physics2D.OverlapAreaAll(new Vector2(-100000f, -100000f), new Vector2(100000f, 100000f), LayerMask.GetMask("Ground"));
            var counts = new SortedDictionary<string, int>();
            foreach (var c in hits) {
                if (c.GetComponent<UnityEngine.Tilemaps.TilemapCollider2D>() != null || c.GetComponent<CompositeCollider2D>() != null) {
                    continue;
                }
                var key = c.gameObject.name + " @" + ((Vector2)c.transform.position).ToString("0");
                counts.TryGetValue(key, out var n);
                counts[key] = n + 1;
            }
            var parts = new List<string>();
            foreach (var kv in counts) {
                parts.Add(kv.Value > 1 ? kv.Key + " x" + kv.Value : kv.Key);
            }
            log.Debug($"ground colliders {when}: {hits.Length} total, non-tile: {string.Join("; ", parts)}");
        }

        static unsafe uint StateHash(UnityEngine.Random.State s) {
            var p = (uint*)&s;
            return p[0] ^ (p[1] * 31) ^ (p[2] * 131) ^ (p[3] * 1031);
        }

        static void Leave() {
            if (depth <= 0) {
                return;
            }
            if (--depth == 0) {
                genState = UnityEngine.Random.state;
                UnityEngine.Random.state = outerState;
            }
        }

        static bool BeginGeneration() {
            var world = WorldGeneration.world;
            if (isClient()) {
                RefreshTarget();
                if (targetBlob == null || targetGen <= appliedGen) {
                    if (!waitingNoted) {
                        waitingNoted = true;
                        waitingSince = Time.realtimeSinceStartup;
                        log.Info("waiting for the host to generate the world...");
                    }
                    if (world != null && world.loadingText != null) {
                        world.loadingText.text = "Waiting for host... " + (int)(Time.realtimeSinceStartup - waitingSince) + "s";
                    }
                    return false;
                }
                waitingNoted = false;
                Apply(world, targetBlob);
                appliedGen = targetGen;
                log.Info($"generating host world #{appliedGen} (layer {world.biomeDepth})");
                return true;
            }
            genState = UnityEngine.Random.state;
            lastBlob = Capture(world, genState);
            if (isHost()) {
                appliedGen = Ct.WorldBegin(lastBlob);
                log.Info($"generating world #{appliedGen} for everyone (layer {world.biomeDepth})");
            } else {
                appliedGen = 0;
            }
            return true;
        }

        public static void OnHostStarted() {
            var world = WorldGeneration.world;
            if (lastBlob == null || world == null || world.generatingWorld) {
                return;
            }
            appliedGen = Ct.WorldBegin(lastBlob);
            Ct.ct_world_ready(appliedGen);
            Items.OnHostStarted();
            log.Warning($"hosting an already generated world #{appliedGen}: tile changes made before hosting are not shared");
        }

        public static void StartRun(PreRunScript prs, Dictionary<string, object> settings) {
            SaveSystem.loadedRun = false;
            if (settings != null) {
                prs.runSettings = settings;
            }
            WorldGeneration.runSettings = prs.runSettings;
            startedLoading.SetValue(prs, true);
            prs.StartCoroutine((IEnumerator)waitLoad.Invoke(prs, null));
        }

        public static void OnSessionEnded() {
            Items.Reset();
            Enemies.Reset();
            appliedGen = 0;
            targetGen = 0;
            targetBlob = null;
            knownRev = uint.MaxValue;
            followRequested = false;
        }

        static void RefreshTarget() {
            var rev = Ct.ct_world_rev();
            if (rev == knownRev) {
                return;
            }
            knownRev = rev;
            targetBlob = Ct.WorldParams(out targetGen);
            if (targetBlob != null && targetBlob.Length == 0) {
                targetBlob = null;
            }
            followRequested = false;
        }

        public static void Update(bool session) {
            var world = WorldGeneration.world;
            var generating = world != null && world.generatingWorld;
            if (wasGenerating && !generating && world != null) {
                OnGenerated(world, session);
            }
            wasGenerating = generating;
            if (!session) {
                return;
            }
            if (isClient()) {
                RefreshTarget();
                Follow(world);
            }
            if (world != null && !generating && appliedGen != 0) {
                ApplyRemoteTiles(world);
                Entities.ApplyRemote();
                if (Together.Verbose && Time.realtimeSinceStartup >= nextHashReport) {
                    nextHashReport = Time.realtimeSinceStartup + 5f;
                    log.Debug($"world #{appliedGen} hash {Hash(world):X16}");
                }
            }
        }

        static List<TileEdit> SettleDiff(WorldGeneration world) {
            var list = new List<TileEdit>();
            var cp = checkpoint;
            checkpoint = null;
            if (cp == null || !(blocksField.GetValue(world) is ushort[,] now) || now.GetLength(0) != cp.GetLength(0) || now.GetLength(1) != cp.GetLength(1)) {
                return list;
            }
            var w = now.GetLength(0);
            var h = now.GetLength(1);
            for (var x = 0; x < w; x++) {
                for (var y = 0; y < h; y++) {
                    if (now[x, y] != cp[x, y]) {
                        list.Add(new TileEdit { X = (ushort)x, Y = (ushort)y, Block = isClient() ? cp[x, y] : now[x, y] });
                    }
                }
            }
            return list;
        }

        static void OnGenerated(WorldGeneration world, bool session) {
            var settle = SettleDiff(world);
            if (session && isClient() && settle.Count > 0) {
                applyingRemote = true;
                try {
                    foreach (var e in settle) {
                        world.SetBlock(new Vector2Int(e.X, e.Y), e.Block);
                    }
                } finally {
                    applyingRemote = false;
                }
                log.Info($"reverted {settle.Count} tiles changed while the world was finishing, host state will follow");
            }
            Items.RegisterLate(session, !isClient());
            var hash = Hash(world);
            log.Info($"world ready: layer {world.biomeDepth}, {world.width}x{world.height}, hash {hash:X16}" + (appliedGen != 0 ? $", #{appliedGen}" : ""));
            if (session && appliedGen == 0 && isHost() && lastBlob != null) {
                appliedGen = Ct.WorldBegin(lastBlob);
                log.Info($"sharing world #{appliedGen} with everyone");
            }
            if (session && appliedGen != 0) {
                Ct.ct_world_ready(appliedGen);
                if (isHost() && settle.Count > 0) {
                    foreach (var e in settle) {
                        Ct.ct_tile_edit(e.X, e.Y, e.Block);
                    }
                    log.Info($"shared {settle.Count} tiles changed while the world was finishing");
                }
            }
        }

        static void Follow(WorldGeneration world) {
            if (targetBlob == null || targetGen <= appliedGen || followRequested || waitingNoted) {
                return;
            }
            var prs = PreRunScript.instance;
            if (world == null) {
                if (prs == null || (bool)startedLoading.GetValue(prs)) {
                    return;
                }
                followRequested = true;
                var info = Peek(targetBlob);
                if (info.Tutorial) {
                    log.Info("host is in the tutorial, starting it");
                    prs.StartTutorial();
                    return;
                }
                log.Info("host started a run, joining it");
                StartRun(prs, info.RunSettings);
                return;
            }
            if (world.generatingWorld || (bool)doingRegen.GetValue(world)) {
                return;
            }
            followRequested = true;
            log.Info($"host moved to world #{targetGen}, regenerating");
            world.StartCoroutine(world.RegenerateWorld());
        }

        static void ApplyRemoteTiles(WorldGeneration world) {
            int n;
            while ((n = Ct.ct_tile_poll(tileBuf, tileBuf.Length)) > 0) {
                applyingRemote = true;
                try {
                    for (var i = 0; i < n; i++) {
                        var e = tileBuf[i];
                        if (e.X < world.width && e.Y < world.height) {
                            world.SetBlock(new Vector2Int(e.X, e.Y), e.Block);
                        }
                    }
                } finally {
                    applyingRemote = false;
                }
                if (n < tileBuf.Length) {
                    break;
                }
            }
        }

        static void SetBlockPostfix(WorldGeneration __instance, Vector2Int pos, ushort block) {
            Record(__instance, pos, block, 1);
        }

        static void SetBlockNoUpdatePostfix(WorldGeneration __instance, Vector2Int pos, ushort block) {
            Record(__instance, pos, block, 2);
        }

        static void Record(WorldGeneration world, Vector2Int pos, ushort block, int margin) {
            if (applyingRemote || appliedGen == 0 || world.generatingWorld || depth > 0) {
                return;
            }
            var x = Mathf.Clamp(pos.x, 0, (int)world.width - margin);
            var y = Mathf.Clamp(pos.y, 0, (int)world.height - margin);
            Ct.ct_tile_edit((ushort)x, (ushort)y, block);
        }

        static unsafe byte[] Capture(WorldGeneration world, UnityEngine.Random.State state) {
            using (var ms = new MemoryStream()) {
                using (var w = new BinaryWriter(ms)) {
                    w.Write(BlobVersion);
                    var p = (byte*)&state;
                    for (var i = 0; i < sizeof(UnityEngine.Random.State); i++) {
                        w.Write(p[i]);
                    }
                    w.Write(world.biomeDepth);
                    w.Write((int)world.biomeOverride);
                    w.Write(world.lootRarityMultiplier);
                    w.Write(world.trapRarityMultiplier);
                    w.Write(world.totalTraveled);
                    w.Write(world.isMercRun);
                    var settings = WorldGeneration.runSettings ?? new Dictionary<string, object>();
                    w.Write(settings.Count);
                    foreach (var kv in settings) {
                        w.Write(kv.Key);
                        WriteValue(w, kv.Value);
                    }
                }
                return ms.ToArray();
            }
        }

        struct BlobInfo {
            public bool Tutorial;
            public Dictionary<string, object> RunSettings;
        }

        static BlobInfo Peek(byte[] blob) {
            var info = new BlobInfo();
            Read(blob, (r, state, depth, ov, loot, trap, traveled, merc) => {
                info.Tutorial = ov == (int)WorldGeneration.OverrideSceneType.Tutorial;
            }, s => info.RunSettings = s);
            return info;
        }

        static void Apply(WorldGeneration world, byte[] blob) {
            Read(blob, (r, state, depth, ov, loot, trap, traveled, merc) => {
                genState = state;
                world.biomeDepth = depth;
                world.biomeOverride = (WorldGeneration.OverrideSceneType)ov;
                world.lootRarityMultiplier = loot;
                world.trapRarityMultiplier = trap;
                world.totalTraveled = traveled;
                world.isMercRun = merc;
            }, s => WorldGeneration.runSettings = s);
        }

        delegate void HeaderFn(BinaryReader r, UnityEngine.Random.State state, int depth, int ov, float loot, float trap, int traveled, bool merc);

        static unsafe void Read(byte[] blob, HeaderFn header, Action<Dictionary<string, object>> settings) {
            using (var r = new BinaryReader(new MemoryStream(blob))) {
                if (r.ReadInt32() != BlobVersion) {
                    throw new InvalidDataException("world blob version mismatch");
                }
                var state = default(UnityEngine.Random.State);
                var p = (byte*)&state;
                for (var i = 0; i < sizeof(UnityEngine.Random.State); i++) {
                    p[i] = r.ReadByte();
                }
                header(r, state, r.ReadInt32(), r.ReadInt32(), r.ReadSingle(), r.ReadSingle(), r.ReadInt32(), r.ReadBoolean());
                var n = r.ReadInt32();
                var dict = new Dictionary<string, object>(n);
                for (var i = 0; i < n; i++) {
                    var key = r.ReadString();
                    dict[key] = ReadValue(r);
                }
                settings(dict);
            }
        }

        static void WriteValue(BinaryWriter w, object v) {
            switch (v) {
                case bool b: w.Write((byte)1); w.Write(b); break;
                case int i: w.Write((byte)2); w.Write(i); break;
                case float f: w.Write((byte)3); w.Write(f); break;
                case double d: w.Write((byte)4); w.Write(d); break;
                case long l: w.Write((byte)5); w.Write(l); break;
                case null: w.Write((byte)0); break;
                default: w.Write((byte)6); w.Write(v.ToString()); break;
            }
        }

        static object ReadValue(BinaryReader r) {
            switch (r.ReadByte()) {
                case 1: return r.ReadBoolean();
                case 2: return r.ReadInt32();
                case 3: return r.ReadSingle();
                case 4: return r.ReadDouble();
                case 5: return r.ReadInt64();
                case 6: return r.ReadString();
                default: return null;
            }
        }

        static unsafe ulong Hash(WorldGeneration world) {
            if (!(blocksField.GetValue(world) is ushort[,] blocks) || blocks.Length == 0) {
                return 0;
            }
            var h = 14695981039346656037UL;
            fixed (ushort* p = &blocks[0, 0]) {
                var n = blocks.Length;
                for (var i = 0; i < n; i++) {
                    h = (h ^ p[i]) * 1099511628211UL;
                }
            }
            return h;
        }
    }
}

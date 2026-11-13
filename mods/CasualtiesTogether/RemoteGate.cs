using System;
using System.Collections.Generic;
using System.Linq;
using System.Reflection;
using System.Reflection.Emit;
using HarmonyLib;
using UnityEngine;

namespace CasualtiesTogether {
    static class RemoteGate {
        static readonly HashSet<Body> remote = new HashSet<Body>();
        static PlayerCamera sink;
        static GameObject sinkRoot;

        static readonly FieldInfo mainField = AccessTools.Field(typeof(PlayerCamera), nameof(PlayerCamera.main));
        static readonly MethodInfo getKey = AccessTools.Method(typeof(Input), nameof(Input.GetKey), new[] { typeof(KeyCode) });
        static readonly MethodInfo gateBody = AccessTools.Method(typeof(RemoteGate), nameof(CameraFor), new[] { typeof(PlayerCamera), typeof(Body) });
        static readonly MethodInfo gateLimb = AccessTools.Method(typeof(RemoteGate), nameof(CameraFor), new[] { typeof(PlayerCamera), typeof(Limb) });
        static readonly MethodInfo gateKey = AccessTools.Method(typeof(RemoteGate), nameof(KeyFor));

        public static int Patched { get; private set; }

        public static Body Context;

        public static PlayerCamera CameraForContext(PlayerCamera main) {
            return Context is null ? main : CameraFor(main, Context);
        }

        public static bool IsRemote(Body b) => b != null && remote.Contains(b);

        public static void Mark(Body b) {
            remote.Add(b);
        }

        public static void Unmark(Body b) {
            remote.Remove(b);
        }

        public static PlayerCamera CameraFor(PlayerCamera main, Body self) {
            if (self is null || remote.Count == 0 || !remote.Contains(self)) {
                return main;
            }
            var s = Sink(main);
            if (s != null) {
                s.body = self;
            }
            return s ?? main;
        }

        public static PlayerCamera CameraFor(PlayerCamera main, Limb self) {
            return self is null ? main : CameraFor(main, self.body);
        }

        public static bool KeyFor(KeyCode key, Body self) {
            return !IsRemote(self) && Input.GetKey(key);
        }

        static PlayerCamera Sink(PlayerCamera main) {
            if (sink != null) {
                return sink;
            }
            if (main == null) {
                return null;
            }
            sinkRoot = new GameObject("CT_CameraSink");
            sinkRoot.SetActive(false);
            UnityEngine.Object.DontDestroyOnLoad(sinkRoot);
            var clone = UnityEngine.Object.Instantiate(main.gameObject, sinkRoot.transform);
            clone.name = "CT_CameraSinkClone";
            sink = clone.GetComponent<PlayerCamera>();
            return sink;
        }

        static bool SinkPrefix(object __instance) {
            return sink is null || !ReferenceEquals(__instance, sink);
        }

        static bool SinkCoroutinePrefix(MonoBehaviour __instance, ref Coroutine __result) {
            if (sink is null || !ReferenceEquals(__instance, sink)) {
                return true;
            }
            __result = null;
            return false;
        }

        public static void Install(Harmony harmony, ModLoader.Logger log) {
            var targets = new List<(MethodBase, MethodInfo)>();
            foreach (var t in new[] { typeof(Body), typeof(Limb) }) {
                var gate = t == typeof(Body) ? gateBody : gateLimb;
                foreach (var m in AccessTools.GetDeclaredMethods(t)) {
                    if (!m.IsStatic && !m.IsAbstract && m.GetMethodBody() != null && Uses(m, t == typeof(Body))) {
                        targets.Add((m, gate));
                    }
                }
                foreach (var nested in t.GetNestedTypes(BindingFlags.NonPublic | BindingFlags.Public)) {
                    if (AccessTools.Field(nested, "<>4__this")?.FieldType != t) {
                        continue;
                    }
                    var moveNext = AccessTools.Method(nested, "MoveNext");
                    if (moveNext != null && Uses(moveNext, t == typeof(Body))) {
                        targets.Add((moveNext, gate));
                    }
                }
            }
            var transpiler = new HarmonyMethod(typeof(RemoteGate), nameof(Transpile));
            foreach (var (m, _) in targets) {
                try {
                    harmony.Patch(m, transpiler: transpiler);
                    Patched++;
                } catch (Exception e) {
                    log.Warning($"gate patch failed on {m.DeclaringType.Name}.{m.Name}: {e.Message}");
                }
            }
            var prefix = new HarmonyMethod(typeof(RemoteGate), nameof(SinkPrefix));
            var skipped = 0;
            foreach (var m in AccessTools.GetDeclaredMethods(typeof(PlayerCamera))) {
                if (m.IsStatic || m.IsAbstract || m.GetMethodBody() == null || m.IsGenericMethodDefinition) {
                    continue;
                }
                try {
                    harmony.Patch(m, prefix: prefix);
                } catch {
                    skipped++;
                }
            }
            var coPrefix = new HarmonyMethod(typeof(RemoteGate), nameof(SinkCoroutinePrefix));
            foreach (var m in typeof(MonoBehaviour).GetMethods(BindingFlags.Public | BindingFlags.Instance)) {
                if (m.Name == nameof(MonoBehaviour.StartCoroutine) && m.ReturnType == typeof(Coroutine)) {
                    try {
                        harmony.Patch(m, prefix: coPrefix);
                    } catch (Exception e) {
                        log.Warning($"sink coroutine patch failed: {e.Message}");
                    }
                }
            }
            log.Info($"remote gate: {Patched} body/limb methods gated, PlayerCamera sink ready ({skipped} methods skipped)");
        }

        static bool Uses(MethodBase m, bool checkKeys) {
            try {
                foreach (var ins in PatchProcessor.GetOriginalInstructions(m)) {
                    if (ins.opcode == OpCodes.Ldsfld && Equals(ins.operand, mainField)) {
                        return true;
                    }
                    if (checkKeys && ins.opcode == OpCodes.Call && Equals(ins.operand, getKey)) {
                        return true;
                    }
                }
            } catch {
            }
            return false;
        }

        static IEnumerable<CodeInstruction> Transpile(IEnumerable<CodeInstruction> instructions, MethodBase original) {
            var owner = original.DeclaringType;
            var isMachine = owner != typeof(Body) && owner != typeof(Limb);
            var self = isMachine ? AccessTools.Field(owner, "<>4__this") : null;
            var selfType = isMachine ? self.FieldType : owner;
            var gate = selfType == typeof(Body) ? gateBody : gateLimb;
            IEnumerable<CodeInstruction> LoadSelf() {
                yield return new CodeInstruction(OpCodes.Ldarg_0);
                if (self != null) {
                    yield return new CodeInstruction(OpCodes.Ldfld, self);
                }
            }
            foreach (var ins in instructions) {
                if (ins.opcode == OpCodes.Ldsfld && Equals(ins.operand, mainField)) {
                    yield return ins;
                    foreach (var l in LoadSelf()) {
                        yield return l;
                    }
                    yield return new CodeInstruction(OpCodes.Call, gate);
                    continue;
                }
                if (selfType == typeof(Body) && ins.opcode == OpCodes.Call && Equals(ins.operand, getKey)) {
                    var first = true;
                    foreach (var l in LoadSelf()) {
                        if (first) {
                            l.labels.AddRange(ins.labels);
                            l.blocks.AddRange(ins.blocks);
                            first = false;
                        }
                        yield return l;
                    }
                    yield return new CodeInstruction(OpCodes.Call, gateKey);
                    continue;
                }
                yield return ins;
            }
        }
    }
}

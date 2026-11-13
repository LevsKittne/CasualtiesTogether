using System;
using System.Diagnostics;
using System.Text;

namespace CasualtiesTogether {
    static class Perf {
        public const int World = 0, Items = 1, Health = 2, Enemies = 3, Medical = 4, Ui = 5, Avatars = 6, Voice = 7, Tick = 8, Notes = 9, Fixed = 10, Gui = 11, Count = 12;
        static readonly string[] names = { "world", "items", "health", "enemies", "medical", "ui", "avatars", "voice", "tick", "notes", "fixed", "gui" };
        static readonly long[] total = new long[Count];
        static readonly long[] worst = new long[Count];
        static readonly long[] bytes = new long[Count];
        static long memAtBegin;
        static int frames;
        static long allocated;
        static long lastMem;
        static int gen0;
        static long started = Stopwatch.GetTimestamp();
        public static bool Enabled;
        public static bool Pending;

        public static long Begin() {
            if (!Enabled) {
                return 0;
            }
            memAtBegin = GC.GetTotalMemory(false);
            return Stopwatch.GetTimestamp();
        }

        public static void End(int bucket, long start) {
            if (!Enabled) {
                return;
            }
            var d = Stopwatch.GetTimestamp() - start;
            var m = GC.GetTotalMemory(false) - memAtBegin;
            if (m > 0) {
                bytes[bucket] += m;
            }
            total[bucket] += d;
            if (d > worst[bucket]) {
                worst[bucket] = d;
            }
        }

        public static void Frame() {
            if (Pending) {
                Pending = false;
                Enabled = true;
                Reset();
            }
            if (!Enabled) {
                return;
            }
            frames++;
            var mem = GC.GetTotalMemory(false);
            if (mem > lastMem && lastMem != 0) {
                allocated += mem - lastMem;
            }
            lastMem = mem;
        }

        public static void Reset() {
            Array.Clear(total, 0, Count);
            Array.Clear(worst, 0, Count);
            Array.Clear(bytes, 0, Count);
            frames = 0;
            allocated = 0;
            lastMem = 0;
            gen0 = GC.CollectionCount(0);
            started = Stopwatch.GetTimestamp();
        }

        public static string Report() {
            var sb = new StringBuilder();
            var f = Math.Max(frames, 1);
            var ms = 1000.0 / Stopwatch.Frequency;
            var secs = (Stopwatch.GetTimestamp() - started) / (double)Stopwatch.Frequency;
            long sum = 0;
            for (var i = 0; i < Count; i++) {
                sum += total[i];
            }
            sb.Append($"perf: {frames} frames in {secs:F1}s ({frames / Math.Max(secs, 0.001):F0} fps), mod {sum * ms / f:F3} ms/frame, alloc {allocated / 1024.0 / f:F2} KB/frame, gen0 GCs {GC.CollectionCount(0) - gen0}");
            for (var i = 0; i < Count; i++) {
                sb.Append($"\n  {names[i],-8} {total[i] * ms / f:F4} ms avg, {worst[i] * ms:F2} ms worst, {bytes[i] / (double)f:F0} B/frame");
            }
            return sb.ToString();
        }
    }
}

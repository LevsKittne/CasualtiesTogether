using System.Collections.Generic;
using UnityEngine;
using UnityEngine.EventSystems;
using UnityEngine.UI;

namespace CasualtiesTogether {
    sealed class CprMinigame : Minigame {
        const float MinRate = 65f;
        const float MaxRate = 220f;
        const float MaxDistance = 3f;
        const int KickstartPresses = 10;
        const float RibChance = 0.01f;
        const float StrengthRequirement = 3f;

        public static bool AutoPress;
        readonly Body patient;
        bool pressed;
        float pressStart;
        float releaseStart = -1f;
        float lastRelease;
        float lastPump = -10f;
        int perfect;
        float shownRate;
        float shownQuality;
        string status = "";

        Body Healer => PlayerCamera.main.body;

        public CprMinigame(Body patient) {
            this.patient = patient;
        }

        public override HandSpriteType HandType() => HandSpriteType.Grasp;

        public override string GuideLocaleString() => "";

        public override bool NeedsItem() => false;

        public static bool Strong(Body b) {
            return b.skills.STR * Mathf.Max(b.limbs[5].totalForce, b.limbs[8].totalForce) > StrengthRequirement;
        }

        public static bool NeedsCpr(Body b) {
            return (b.heartRate < 50f && b.bloodOxygen < 100f && b.bloodPressure < 30f) || b.heartRate < 5f || b.isDying || b.isCriticallyDying;
        }

        public override void Start() {
            game.CreateScreen("Special/SelfHarmMinigame");
            if (game.spawnedMiniGame != null) {
                foreach (var img in game.spawnedMiniGame.GetComponentsInChildren<Image>()) {
                    var c = img.color;
                    c.a = 0f;
                    img.color = c;
                }
            }
            UpdateText();
        }

        public override void Update(List<RaycastResult> uiCasts) {
            var healer = Healer;
            if (patient == null || healer == null || !healer.standing || !healer.conscious
                || Vector2.Distance(patient.transform.position, healer.transform.position) > MaxDistance
                || (patient.standing && patient.moveDir != Vector2.zero)) {
                game.EndMinigame();
                return;
            }
            var now = Time.time;
            var down = AutoPress ? (now % 0.55f) < 0.27f : Input.GetMouseButton(0);
            if (down && !pressed) {
                pressed = true;
                pressStart = now;
                if (releaseStart > 0f) {
                    lastRelease = now - releaseStart;
                }
            } else if (!down && pressed) {
                pressed = false;
                releaseStart = now;
                Pump(healer, now - pressStart, lastRelease, now);
            }
            game.handPos = Vector2.Lerp(game.handPos, pressed ? new Vector2(0f, -40f) : Vector2.zero, 0.5f);
            UpdateText();
        }

        void Pump(Body healer, float hold, float release, float now) {
            healer.stamina -= 1.5f;
            if (release <= 0f || hold <= 0f) {
                return;
            }
            if (!Strong(healer)) {
                status = "too weak to compress the chest";
                return;
            }
            var full = hold + release;
            var rate = 60f / full;
            shownRate = rate;
            if (rate < MinRate || rate > MaxRate) {
                perfect = 0;
                status = rate < MinRate ? "too slow" : "too fast";
                return;
            }
            perfect = now - lastPump > 2f ? 0 : perfect + 1;
            lastPump = now;
            var quality = Mathf.Clamp01(1f - Mathf.Abs(hold - release));
            var torso = patient.limbs[1];
            if (torso.broken) {
                quality = Mathf.Lerp(quality, 1f, 0.5f);
            }
            shownQuality = quality;
            if (quality <= 0.5f) {
                status = "uneven presses";
                return;
            }
            status = "";
            var id = Avatars.IdOf(patient);
            if (id >= 0) {
                var payload = new byte[2 + 4 * 6];
                payload[0] = (byte)id;
                payload[1] = (byte)(id >> 8);
                System.Buffer.BlockCopy(System.BitConverter.GetBytes(hold), 0, payload, 2, 4);
                System.Buffer.BlockCopy(System.BitConverter.GetBytes(rate), 0, payload, 6, 4);
                System.Buffer.BlockCopy(System.BitConverter.GetBytes(quality), 0, payload, 10, 4);
                System.Buffer.BlockCopy(System.BitConverter.GetBytes(perfect), 0, payload, 14, 4);
                System.Buffer.BlockCopy(System.BitConverter.GetBytes(healer.skills.STR), 0, payload, 18, 4);
                System.Buffer.BlockCopy(System.BitConverter.GetBytes(healer.skills.INT), 0, payload, 22, 4);
                Ct.EntityCall(0, OpPump, 0, (uint)Random.Range(1, int.MaxValue), payload);
            }
            Sound.Play("switch", patient.transform.position);
        }

        public const ushort OpPump = 0xF103;

        public static void ApplyPump(byte[] buf, int offset, int len) {
            if (len < 26) {
                return;
            }
            var target = (ushort)(buf[offset] | (buf[offset + 1] << 8));
            var patient = PlayerCamera.main != null ? PlayerCamera.main.body : null;
            if (target != Ct.ct_local_id() || patient == null) {
                return;
            }
            var hold = System.BitConverter.ToSingle(buf, offset + 2);
            var rate = System.BitConverter.ToSingle(buf, offset + 6);
            var quality = System.BitConverter.ToSingle(buf, offset + 10);
            var perfect = System.BitConverter.ToInt32(buf, offset + 14);
            var str = System.BitConverter.ToInt32(buf, offset + 18);
            var intel = System.BitConverter.ToInt32(buf, offset + 22);
            var torso = patient.limbs[1];
            var pump = patient.bloodVolume > 0f ? Mathf.Clamp(patient.bloodVolume, 30f, 110f) * Mathf.Clamp(hold * 1.13f, 0.06f, 0.3f) * quality : 0f;
            if (patient.bloodOxygen < 50f) {
                patient.bloodOxygen += pump * 0.7f;
            }
            if (patient.bloodPressure < 100f) {
                patient.bloodPressure += pump;
            }
            var center = (MinRate + MaxRate) * 0.5f;
            if (rate > center && quality < 0.9f && !torso.broken && str > 8) {
                var chance = (str - intel * 0.7f) * RibChance;
                if (chance > 0f && Random.value < chance) {
                    torso.BreakBone();
                    patient.internalBleeding += 5f + chance;
                }
            }
            var restarted = false;
            if (patient.bloodPressure > 80f && perfect > KickstartPresses && patient.alive && patient.inCardiacArrest && patient.bloodVolume > 10f
                && Random.value * quality > 0.8f) {
                patient.heartRate = Mathf.Max(patient.heartRate, Mathf.LerpUnclamped(patient.heartRate, rate * 0.6f, quality));
                restarted = true;
            }
            if (Together.Verbose || restarted) {
                Together.Log?.Write(restarted ? ModLoader.LogLevel.Info : ModLoader.LogLevel.Debug,
                    $"CPR press: pump {pump:0.0}, heart {patient.heartRate:0}, pressure {patient.bloodPressure:0}, oxygen {patient.bloodOxygen:0}" + (restarted ? ", heartbeat restarted" : ""));
            }
        }

        void UpdateText() {
            if (game.guideText == null) {
                return;
            }
            var rate = shownRate > 0f ? $"{shownRate:0}/min" : "-";
            game.guideText.text = $"CPR: press and release the mouse evenly, about 100-120 times per minute\nrate {rate}, quality {shownQuality * 100f:0}%, streak {perfect}"
                + (status.Length > 0 ? "\n" + status : "");
        }
    }
}

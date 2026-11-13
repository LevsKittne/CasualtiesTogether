[English](../README.md) | [Русский](README.ru.md) | [Українська](README.uk.md) | [简体中文](README.zh-CN.md) | [Español](README.es.md) | [Português](README.pt-BR.md) | [Deutsch](README.de.md) | [Français](README.fr.md) | [日本語](README.ja.md) | [한국어](README.ko.md) | **Türkçe** | [Polski](README.pl.md)

# Casualties: Together

Casualties Unknown için çok oyunculu (co-op) mod. [CU ModLoader](https://github.com/LevsKittne/CUModLoader) gerektirir.

## Özellikler

- Sunucu (host), istemci ve özel sunucu; Steam P2P veya doğrudan UDP (IP:port).
- Dünya üretimi, karolar, eşyalar, envanter, düşmanlar ve yapılar senkronize.
- Tıp: vücut durumu, diğer oyuncuları tedavi, CPR.
- Sesli sohbet (Opus), bas-konuş veya açık mikrofon, mesafeye bağlı.
- Sohbet, oyuncu listesi, yasaklar, şifre, sunucu kuralları.
- Değiştiriciler: FriendlyFire, OneHit, SoulLink, SharedStats, SharedStash, Collision, yerçekimi.
- Host devri, host düşünce otomatik geçiş, kayıt aktarımı.
- Değiştirilebilir tuşlar.

## Kurulum

1. CU ModLoader'ı kurun.
2. Releases bölümünden `CasualtiesTogether-Setup.exe` dosyasını indirip herhangi bir yerden çalıştırın.
3. Oyunu bulur, modu `ModLoader\mods\CasualtiesTogether` klasörüne koyar ve kendini siler.

Her oyuncuda mod olmalı ve herkesin oyun sürümü aynı olmalı.

## Kaldırma

`ModLoader\mods\CasualtiesTogether` klasörünü silin.

## Sorun giderme

- "Dosyalar yazılamadı" — oyunu kapatın; olmazsa yükleyiciyi yönetici olarak çalıştırın.
- Windows SmartScreen — "Ek bilgi" → "Yine de çalıştır" (dosyalar imzalı değil).
- `vcruntime140.dll` hatası — Microsoft Visual C++ Redistributable (x64) kurun.
- Günlük: `ModLoader\log.txt`

Kaynak koddan derleme ve mod yazma için İngilizce README'ye bakın.

## Lisans

LGPL-2.1. Mantığın bir kısmı [casualties-together](https://github.com/creaturefeaturelarry/casualties-together) (LGPL-2.1) projesinden aktarılmıştır.

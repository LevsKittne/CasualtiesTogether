[English](../README.md) | [Русский](README.ru.md) | [Українська](README.uk.md) | [简体中文](README.zh-CN.md) | [Español](README.es.md) | [Português](README.pt-BR.md) | **Deutsch** | [Français](README.fr.md) | [日本語](README.ja.md) | [한국어](README.ko.md) | [Türkçe](README.tr.md) | [Polski](README.pl.md)

# Casualties: Together

Multiplayer-Mod (Koop) für Casualties Unknown. Benötigt [CU ModLoader](https://github.com/LevsKittne/CUModLoader).

## Funktionen

- Host, Client und dedizierter Server; Steam P2P oder direktes UDP (IP:Port).
- Synchronisierte Weltgenerierung, Tiles, Gegenstände, Inventar, Gegner und Gebäude.
- Medizin: Körperzustand, andere Spieler behandeln, HLW.
- Sprachchat (Opus), Push-to-Talk oder offenes Mikrofon, entfernungsabhängig.
- Chat, Spielerliste, Banns, Passwort, Serverregeln.
- Modifikatoren: FriendlyFire, OneHit, SoulLink, SharedStats, SharedStash, Collision, Schwerkraft.
- Host-Übergabe, Host-Migration, Übertragung des Spielstands.
- Frei belegbare Tasten.

## Installation

1. Installiere CU ModLoader.
2. Lade `CasualtiesTogether-Setup.exe` unter Releases herunter und starte es von beliebigem Ort.
3. Es findet das Spiel, legt den Mod in `ModLoader\mods\CasualtiesTogether` und löscht sich selbst.

Jeder Spieler braucht den Mod und dieselbe Spielversion.

## Deinstallation

Lösche den Ordner `ModLoader\mods\CasualtiesTogether`.

## Fehlerbehebung

- „Dateien konnten nicht geschrieben werden“ — Spiel schließen; hilft das nicht, Installer als Administrator starten.
- Windows SmartScreen — „Weitere Informationen“ → „Trotzdem ausführen“ (die Dateien sind nicht signiert).
- Fehler zu `vcruntime140.dll` — Microsoft Visual C++ Redistributable (x64) installieren.
- Log: `ModLoader\log.txt`.

Bauen aus dem Quellcode und Mods schreiben: siehe englisches README.

## Lizenz

LGPL-2.1. Ein Teil der Logik ist aus [casualties-together](https://github.com/creaturefeaturelarry/casualties-together) (LGPL-2.1) portiert.

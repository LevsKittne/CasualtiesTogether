**English** | [Русский](docs/README.ru.md) | [Українська](docs/README.uk.md) | [简体中文](docs/README.zh-CN.md) | [Español](docs/README.es.md) | [Português](docs/README.pt-BR.md) | [Deutsch](docs/README.de.md) | [Français](docs/README.fr.md) | [日本語](docs/README.ja.md) | [한국어](docs/README.ko.md) | [Türkçe](docs/README.tr.md) | [Polski](docs/README.pl.md)

# Casualties: Together

Multiplayer / co-op mod for **Casualties Unknown**. Play Casualties Unknown online with friends. Runs on [CU ModLoader](https://github.com/LevsKittne/CUModLoader).

The core is written in Rust (`ct.dll`): networking, protocol, sessions, voice chat, egui overlay. A thin C# layer (`CasualtiesTogether.dll`) applies Harmony patches to the game and moves state to the core.

## Features

- Host, client and dedicated server; Steam P2P or direct UDP (IP:port).
- Synced world generation, tiles, items, inventory, enemies and buildings.
- Medicine: body state, treating other players, CPR.
- Voice chat (Opus), push-to-talk or open mic, proximity.
- Chat, player list, bans, password, server rules.
- Modifiers: FriendlyFire, OneHit, SoulLink, SharedStats, SharedStash, Collision, gravity.
- Host transfer, host migration when the host drops, save transfer.
- Rebindable keys.

## Installation

1. Install [CU ModLoader](https://github.com/LevsKittne/CUModLoader).
2. Download `CasualtiesTogether-Setup.exe` from [Releases](https://github.com/LevsKittne/CasualtiesTogether/releases) and run it from anywhere.
3. It finds the game, puts the mod into `ModLoader\mods\CasualtiesTogether` and deletes itself.

Every player needs the mod, and everyone must have the same game version. The installer speaks 12 languages (picked from the Windows language).

## Uninstall

Delete the `ModLoader\mods\CasualtiesTogether` folder.

## Troubleshooting

- "Could not write files": close the game; if that does not help, run the installer as administrator.
- Windows SmartScreen: "More info" → "Run anyway" (the files are not signed).
- Error about `vcruntime140.dll`: install Microsoft Visual C++ Redistributable (x64).
- Log: `ModLoader\log.txt`.

## Building

Requires Rust (stable, MSVC), .NET SDK and the game with CU ModLoader installed (the build takes `ModLoader.dll` and `0Harmony.dll` from `ModLoader\core`).

```powershell
.\build.ps1 -GameDir "C:\...\Casualties Unknown"
.\build.ps1 -Install
```

## License

LGPL-2.1, see [LICENSE](LICENSE).

Part of the logic is ported from [casualties-together](https://github.com/creaturefeaturelarry/casualties-together) (LGPL-2.1).

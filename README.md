# Casualties: Together

Мультиплеер для **Casualties Unknown**. Работает как мод для [CU ModLoader](https://github.com/LevsKittne/CUModLoader).

Ядро написано на Rust (`ct.dll`): сеть, протокол, сессии, голосовой чат, оверлей на egui. Тонкий C#-слой (`CasualtiesTogether.dll`) ставит Harmony-патчи в игру и передаёт состояние в ядро.

## Возможности

- Хост, клиент и выделенный сервер. Подключение через Steam P2P или прямой UDP (IP:порт).
- Синхронизация генерации мира, тайлов, предметов, инвентаря, врагов и строений.
- Медицина: состояние тела, лечение других игроков, CPR.
- Голосовой чат (Opus), push-to-talk или открытый микрофон, звук зависит от расстояния.
- Чат, список игроков, баны, пароль, правила сервера.
- Модификаторы: FriendlyFire, OneHit, SoulLink, SharedStats, SharedStash, Collision, множитель гравитации.
- Передача хоста и миграция при вылете хоста, передача сохранения.
- Переназначаемые клавиши.

## Установка

Сначала нужен установленный [CU ModLoader](https://github.com/LevsKittne/CUModLoader).

1. Скачайте `CasualtiesTogether-Setup.exe` из Releases.
2. Запустите его. Он сам найдёт игру и положит мод в `ModLoader\mods\CasualtiesTogether`, после чего удалит себя.

Для игры по сети мод должен стоять у всех игроков, и версия игры у всех должна совпадать.

## Сборка

Нужны Rust (stable, MSVC), .NET SDK, а в игре должен стоять CU ModLoader: сборка берёт `ModLoader.dll` и `0Harmony.dll` из `ModLoader\core`.

```powershell
.\build.ps1 -GameDir "C:\...\Casualties Unknown"
.\build.ps1 -Install
```

## Лицензия

LGPL-2.1, см. [LICENSE](LICENSE).

Источник кода: часть логики портирована из [casualties-together](https://github.com/creaturefeaturelarry/casualties-together) (LGPL-2.1).

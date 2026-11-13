[English](../README.md) | **Русский** | [Українська](README.uk.md) | [简体中文](README.zh-CN.md) | [Español](README.es.md) | [Português](README.pt-BR.md) | [Deutsch](README.de.md) | [Français](README.fr.md) | [日本語](README.ja.md) | [한국어](README.ko.md) | [Türkçe](README.tr.md) | [Polski](README.pl.md)

# Casualties: Together

Мультиплеер (кооператив) для Casualties Unknown. Нужен [CU ModLoader](https://github.com/LevsKittne/CUModLoader).

## Возможности

- Хост, клиент и выделенный сервер; Steam P2P или прямой UDP (IP:порт).
- Синхронизация генерации мира, тайлов, предметов, инвентаря, врагов и построек.
- Медицина: состояние тела, лечение других игроков, СЛР.
- Голосовой чат (Opus), push-to-talk или открытый микрофон, звук по расстоянию.
- Чат, список игроков, баны, пароль, правила сервера.
- Модификаторы: FriendlyFire, OneHit, SoulLink, SharedStats, SharedStash, Collision, гравитация.
- Передача хоста, миграция хоста при вылете, передача сохранения.
- Переназначаемые клавиши.

## Установка

1. Установите CU ModLoader.
2. Скачайте `CasualtiesTogether-Setup.exe` из Releases и запустите его откуда угодно.
3. Он найдёт игру, положит мод в `ModLoader\mods\CasualtiesTogether` и удалит себя.

Мод нужен каждому игроку, и версия игры у всех должна совпадать.

## Удаление

Удалите папку `ModLoader\mods\CasualtiesTogether`.

## Если что-то не так

- «Не удалось записать файлы» — закройте игру; если не помогло, запустите установщик от имени администратора.
- Windows SmartScreen — «Подробнее» → «Выполнить в любом случае» (файлы не подписаны).
- Ошибка про `vcruntime140.dll` — установите Microsoft Visual C++ Redistributable (x64).
- Лог: `ModLoader\log.txt`.

Сборка из исходников и создание модов — см. английский README.

## Лицензия

LGPL-2.1. Часть логики портирована из [casualties-together](https://github.com/creaturefeaturelarry/casualties-together) (LGPL-2.1).

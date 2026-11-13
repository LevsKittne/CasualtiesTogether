[English](../README.md) | [Русский](README.ru.md) | **Українська** | [简体中文](README.zh-CN.md) | [Español](README.es.md) | [Português](README.pt-BR.md) | [Deutsch](README.de.md) | [Français](README.fr.md) | [日本語](README.ja.md) | [한국어](README.ko.md) | [Türkçe](README.tr.md) | [Polski](README.pl.md)

# Casualties: Together

Мультиплеєр (кооператив) для Casualties Unknown. Потрібен [CU ModLoader](https://github.com/LevsKittne/CUModLoader).

## Можливості

- Хост, клієнт і виділений сервер; Steam P2P або прямий UDP (IP:порт).
- Синхронізація генерації світу, тайлів, предметів, інвентарю, ворогів і будівель.
- Медицина: стан тіла, лікування інших гравців, СЛР.
- Голосовий чат (Opus), push-to-talk або відкритий мікрофон, звук за відстанню.
- Чат, список гравців, бани, пароль, правила сервера.
- Модифікатори: FriendlyFire, OneHit, SoulLink, SharedStats, SharedStash, Collision, гравітація.
- Передача хоста, міграція хоста при вильоті, передача збереження.
- Перепризначувані клавіші.

## Встановлення

1. Встановіть CU ModLoader.
2. Завантажте `CasualtiesTogether-Setup.exe` з Releases і запустіть звідки завгодно.
3. Він знайде гру, покладе мод у `ModLoader\mods\CasualtiesTogether` і видалить себе.

Мод потрібен кожному гравцю, і версія гри в усіх має збігатися.

## Видалення

Видаліть папку `ModLoader\mods\CasualtiesTogether`.

## Якщо щось не так

- «Не вдалося записати файли» — закрийте гру; якщо не допомогло, запустіть інсталятор від імені адміністратора.
- Windows SmartScreen — «Докладніше» → «Однаково запустити» (файли не підписані).
- Помилка про `vcruntime140.dll` — встановіть Microsoft Visual C++ Redistributable (x64).
- Лог: `ModLoader\log.txt`.

Збирання з вихідного коду та створення модів — див. англійський README.

## Ліцензія

LGPL-2.1. Частину логіки портовано з [casualties-together](https://github.com/creaturefeaturelarry/casualties-together) (LGPL-2.1).

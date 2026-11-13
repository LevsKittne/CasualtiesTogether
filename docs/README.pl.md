[English](../README.md) | [Русский](README.ru.md) | [Українська](README.uk.md) | [简体中文](README.zh-CN.md) | [Español](README.es.md) | [Português](README.pt-BR.md) | [Deutsch](README.de.md) | [Français](README.fr.md) | [日本語](README.ja.md) | [한국어](README.ko.md) | [Türkçe](README.tr.md) | **Polski**

# Casualties: Together

Mod wieloosobowy (co-op) do Casualties Unknown. Wymaga [CU ModLoader](https://github.com/LevsKittne/CUModLoader).

## Funkcje

- Host, klient i serwer dedykowany; Steam P2P lub bezpośredni UDP (IP:port).
- Synchronizacja generowania świata, kafelków, przedmiotów, ekwipunku, wrogów i budowli.
- Medycyna: stan ciała, leczenie innych graczy, RKO.
- Czat głosowy (Opus), push-to-talk lub otwarty mikrofon, zależny od odległości.
- Czat, lista graczy, bany, hasło, zasady serwera.
- Modyfikatory: FriendlyFire, OneHit, SoulLink, SharedStats, SharedStash, Collision, grawitacja.
- Przekazanie hosta, migracja hosta po rozłączeniu, przesyłanie zapisu.
- Zmiana przypisania klawiszy.

## Instalacja

1. Zainstaluj CU ModLoader.
2. Pobierz `CasualtiesTogether-Setup.exe` z Releases i uruchom z dowolnego miejsca.
3. Znajdzie grę, umieści mod w `ModLoader\mods\CasualtiesTogether` i sam się usunie.

Każdy gracz potrzebuje moda i tej samej wersji gry.

## Odinstalowanie

Usuń folder `ModLoader\mods\CasualtiesTogether`.

## Rozwiązywanie problemów

- „Nie udało się zapisać plików” — zamknij grę; jeśli nie pomoże, uruchom instalator jako administrator.
- Windows SmartScreen — „Więcej informacji” → „Uruchom mimo to” (pliki nie są podpisane).
- Błąd `vcruntime140.dll` — zainstaluj Microsoft Visual C++ Redistributable (x64).
- Log: `ModLoader\log.txt`.

Budowanie ze źródeł i tworzenie modów — zobacz angielskie README.

## Licencja

LGPL-2.1. Część logiki przeniesiono z [casualties-together](https://github.com/creaturefeaturelarry/casualties-together) (LGPL-2.1).

[English](../README.md) | [Русский](README.ru.md) | [Українська](README.uk.md) | [简体中文](README.zh-CN.md) | [Español](README.es.md) | [Português](README.pt-BR.md) | [Deutsch](README.de.md) | **Français** | [日本語](README.ja.md) | [한국어](README.ko.md) | [Türkçe](README.tr.md) | [Polski](README.pl.md)

# Casualties: Together

Mod multijoueur (coop) pour Casualties Unknown. Nécessite [CU ModLoader](https://github.com/LevsKittne/CUModLoader).

## Fonctionnalités

- Hôte, client et serveur dédié ; Steam P2P ou UDP direct (IP:port).
- Synchronisation de la génération du monde, des tuiles, objets, inventaire, ennemis et bâtiments.
- Médecine : état du corps, soigner les autres joueurs, RCP.
- Chat vocal (Opus), appuyer pour parler ou micro ouvert, selon la distance.
- Chat, liste des joueurs, bannissements, mot de passe, règles du serveur.
- Modificateurs : FriendlyFire, OneHit, SoulLink, SharedStats, SharedStash, Collision, gravité.
- Transfert et migration de l'hôte, transfert de la sauvegarde.
- Touches réassignables.

## Installation

1. Installez CU ModLoader.
2. Téléchargez `CasualtiesTogether-Setup.exe` depuis Releases et lancez-le depuis n'importe où.
3. Il trouve le jeu, place le mod dans `ModLoader\mods\CasualtiesTogether` puis se supprime.

Chaque joueur a besoin du mod et de la même version du jeu.

## Désinstallation

Supprimez le dossier `ModLoader\mods\CasualtiesTogether`.

## Dépannage

- « Impossible d'écrire les fichiers » — fermez le jeu ; sinon, lancez l'installateur en tant qu'administrateur.
- Windows SmartScreen — « Informations complémentaires » → « Exécuter quand même » (fichiers non signés).
- Erreur concernant `vcruntime140.dll` — installez Microsoft Visual C++ Redistributable (x64).
- Journal : `ModLoader\log.txt`.

Pour compiler depuis les sources et écrire des mods, voir le README en anglais.

## Licence

LGPL-2.1. Une partie de la logique est portée depuis [casualties-together](https://github.com/creaturefeaturelarry/casualties-together) (LGPL-2.1).

[English](../README.md) | [Русский](README.ru.md) | [Українська](README.uk.md) | [简体中文](README.zh-CN.md) | **Español** | [Português](README.pt-BR.md) | [Deutsch](README.de.md) | [Français](README.fr.md) | [日本語](README.ja.md) | [한국어](README.ko.md) | [Türkçe](README.tr.md) | [Polski](README.pl.md)

# Casualties: Together

Mod multijugador (cooperativo) para Casualties Unknown. Requiere [CU ModLoader](https://github.com/LevsKittne/CUModLoader).

## Características

- Anfitrión, cliente y servidor dedicado; Steam P2P o UDP directo (IP:puerto).
- Sincronización de la generación del mundo, casillas, objetos, inventario, enemigos y construcciones.
- Medicina: estado del cuerpo, curar a otros jugadores, RCP.
- Chat de voz (Opus), pulsar para hablar o micrófono abierto, por proximidad.
- Chat, lista de jugadores, baneos, contraseña, reglas del servidor.
- Modificadores: FriendlyFire, OneHit, SoulLink, SharedStats, SharedStash, Collision, gravedad.
- Transferencia y migración del anfitrión, transferencia de la partida guardada.
- Teclas reasignables.

## Instalación

1. Instala CU ModLoader.
2. Descarga `CasualtiesTogether-Setup.exe` desde Releases y ejecútalo desde cualquier lugar.
3. Busca el juego, pone el mod en `ModLoader\mods\CasualtiesTogether` y se elimina solo.

Todos los jugadores necesitan el mod y la misma versión del juego.

## Desinstalación

Borra la carpeta `ModLoader\mods\CasualtiesTogether`.

## Solución de problemas

- «No se pudieron escribir los archivos»: cierra el juego; si no funciona, ejecuta el instalador como administrador.
- Windows SmartScreen: «Más información» → «Ejecutar de todas formas» (los archivos no están firmados).
- Error sobre `vcruntime140.dll`: instala Microsoft Visual C++ Redistributable (x64).
- Registro: `ModLoader\log.txt`.

Para compilar desde el código fuente y crear mods, consulta el README en inglés.

## Licencia

LGPL-2.1. Parte de la lógica está portada de [casualties-together](https://github.com/creaturefeaturelarry/casualties-together) (LGPL-2.1).

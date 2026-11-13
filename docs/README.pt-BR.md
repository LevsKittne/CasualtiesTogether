[English](../README.md) | [Русский](README.ru.md) | [Українська](README.uk.md) | [简体中文](README.zh-CN.md) | [Español](README.es.md) | **Português** | [Deutsch](README.de.md) | [Français](README.fr.md) | [日本語](README.ja.md) | [한국어](README.ko.md) | [Türkçe](README.tr.md) | [Polski](README.pl.md)

# Casualties: Together

Mod multijogador (cooperativo) para Casualties Unknown. Requer o [CU ModLoader](https://github.com/LevsKittne/CUModLoader).

## Recursos

- Host, cliente e servidor dedicado; Steam P2P ou UDP direto (IP:porta).
- Sincronização da geração do mundo, blocos, itens, inventário, inimigos e construções.
- Medicina: estado do corpo, tratar outros jogadores, RCP.
- Chat de voz (Opus), apertar para falar ou microfone aberto, por proximidade.
- Chat, lista de jogadores, banimentos, senha, regras do servidor.
- Modificadores: FriendlyFire, OneHit, SoulLink, SharedStats, SharedStash, Collision, gravidade.
- Transferência e migração do host, transferência do save.
- Teclas reconfiguráveis.

## Instalação

1. Instale o CU ModLoader.
2. Baixe `CasualtiesTogether-Setup.exe` em Releases e execute de qualquer lugar.
3. Ele encontra o jogo, coloca o mod em `ModLoader\mods\CasualtiesTogether` e se exclui.

Todos os jogadores precisam do mod e da mesma versão do jogo.

## Desinstalação

Apague a pasta `ModLoader\mods\CasualtiesTogether`.

## Solução de problemas

- “Não foi possível gravar os arquivos” — feche o jogo; se não resolver, execute o instalador como administrador.
- Windows SmartScreen — “Mais informações” → “Executar assim mesmo” (os arquivos não são assinados).
- Erro sobre `vcruntime140.dll` — instale o Microsoft Visual C++ Redistributable (x64).
- Log: `ModLoader\log.txt`.

Para compilar a partir do código-fonte e criar mods, veja o README em inglês.

## Licença

LGPL-2.1. Parte da lógica foi portada de [casualties-together](https://github.com/creaturefeaturelarry/casualties-together) (LGPL-2.1).

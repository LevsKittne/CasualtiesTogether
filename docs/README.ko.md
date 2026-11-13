[English](../README.md) | [Русский](README.ru.md) | [Українська](README.uk.md) | [简体中文](README.zh-CN.md) | [Español](README.es.md) | [Português](README.pt-BR.md) | [Deutsch](README.de.md) | [Français](README.fr.md) | [日本語](README.ja.md) | **한국어** | [Türkçe](README.tr.md) | [Polski](README.pl.md)

# Casualties: Together

Casualties Unknown용 멀티플레이(협동) 모드입니다. [CU ModLoader](https://github.com/LevsKittne/CUModLoader)가 필요합니다.

## 기능

- 호스트, 클라이언트, 전용 서버. Steam P2P 또는 직접 UDP(IP:포트).
- 월드 생성, 타일, 아이템, 인벤토리, 적, 건물 동기화.
- 의료: 신체 상태, 다른 플레이어 치료, 심폐소생술(CPR).
- 음성 채팅(Opus), 눌러서 말하기 또는 열린 마이크, 거리 기반.
- 채팅, 플레이어 목록, 밴, 비밀번호, 서버 규칙.
- 모디파이어: FriendlyFire, OneHit, SoulLink, SharedStats, SharedStash, Collision, 중력 배율.
- 호스트 넘기기, 호스트 이탈 시 이전, 세이브 전송.
- 키 재지정.

## 설치

1. CU ModLoader를 설치합니다.
2. Releases에서 `CasualtiesTogether-Setup.exe`를 내려받아 아무 곳에서나 실행합니다.
3. 게임을 찾아 모드를 `ModLoader\mods\CasualtiesTogether`에 넣고 스스로 삭제됩니다.

모든 플레이어에게 모드가 필요하며 게임 버전이 같아야 합니다.

## 제거

`ModLoader\mods\CasualtiesTogether` 폴더를 삭제합니다.

## 문제 해결

- "파일을 쓸 수 없습니다" — 게임을 종료하세요. 그래도 안 되면 관리자 권한으로 실행하세요.
- Windows SmartScreen — "추가 정보" → "실행" (파일이 서명되지 않았습니다).
- `vcruntime140.dll` 오류 — Microsoft Visual C++ Redistributable (x64)를 설치하세요.
- 로그: `ModLoader\log.txt`

소스 빌드와 모드 제작은 영어 README를 참고하세요.

## 라이선스

LGPL-2.1. 로직 일부는 [casualties-together](https://github.com/creaturefeaturelarry/casualties-together)(LGPL-2.1)에서 이식했습니다.

[English](../README.md) | [Русский](README.ru.md) | [Українська](README.uk.md) | [简体中文](README.zh-CN.md) | [Español](README.es.md) | [Português](README.pt-BR.md) | [Deutsch](README.de.md) | [Français](README.fr.md) | **日本語** | [한국어](README.ko.md) | [Türkçe](README.tr.md) | [Polski](README.pl.md)

# Casualties: Together

Casualties Unknown 用のマルチプレイ（協力プレイ）MOD です。[CU ModLoader](https://github.com/LevsKittne/CUModLoader) が必要です。

## 機能

- ホスト、クライアント、専用サーバー。Steam P2P または直接 UDP（IP:ポート）。
- ワールド生成、タイル、アイテム、インベントリ、敵、建物の同期。
- 医療：身体状態、他プレイヤーの治療、心肺蘇生（CPR）。
- ボイスチャット（Opus）、プッシュトゥトークまたはオープンマイク、距離による音量。
- チャット、プレイヤーリスト、BAN、パスワード、サーバールール。
- モディファイア：FriendlyFire、OneHit、SoulLink、SharedStats、SharedStash、Collision、重力倍率。
- ホストの譲渡、ホスト切断時の移行、セーブデータの転送。
- キー割り当ての変更。

## インストール

1. CU ModLoader をインストールします。
2. Releases から `CasualtiesTogether-Setup.exe` をダウンロードし、どこからでも実行します。
3. ゲームを見つけて MOD を `ModLoader\mods\CasualtiesTogether` に置き、自動的に削除されます。

全員が MOD を導入し、同じバージョンのゲームを使う必要があります。

## アンインストール

`ModLoader\mods\CasualtiesTogether` フォルダーを削除します。

## トラブルシューティング

- 「ファイルを書き込めませんでした」— ゲームを終了してください。解決しない場合は管理者として実行してください。
- Windows SmartScreen — 「詳細情報」→「実行」（ファイルは署名されていません）。
- `vcruntime140.dll` のエラー — Microsoft Visual C++ Redistributable (x64) をインストールしてください。
- ログ：`ModLoader\log.txt`

ソースからのビルドと MOD の作り方は英語版 README を参照してください。

## ライセンス

LGPL-2.1。ロジックの一部は [casualties-together](https://github.com/creaturefeaturelarry/casualties-together)（LGPL-2.1）から移植しています。

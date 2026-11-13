[English](../README.md) | [Русский](README.ru.md) | [Українська](README.uk.md) | **简体中文** | [Español](README.es.md) | [Português](README.pt-BR.md) | [Deutsch](README.de.md) | [Français](README.fr.md) | [日本語](README.ja.md) | [한국어](README.ko.md) | [Türkçe](README.tr.md) | [Polski](README.pl.md)

# Casualties: Together

Casualties Unknown 的多人联机（合作）模组。需要先安装 [CU ModLoader](https://github.com/LevsKittne/CUModLoader)。

## 功能

- 主机、客户端和专用服务器；支持 Steam P2P 或直接 UDP（IP:端口）。
- 同步世界生成、地块、物品、背包、敌人和建筑。
- 医疗：身体状态同步、治疗其他玩家、心肺复苏（CPR）。
- 语音聊天（Opus），按键说话或开放麦克风，按距离衰减。
- 聊天、玩家列表、封禁、密码、服务器规则。
- 规则修改器：FriendlyFire、OneHit、SoulLink、SharedStats、SharedStash、Collision、重力倍率。
- 转移主机、主机掉线自动迁移、存档传输。
- 可自定义按键。

## 安装

1. 安装 CU ModLoader。
2. 从 Releases 下载 `CasualtiesTogether-Setup.exe`，在任意位置运行。
3. 它会找到游戏，把模组放入 `ModLoader\mods\CasualtiesTogether`，然后自动删除自身。

每位玩家都需要安装此模组，并且所有人的游戏版本必须一致。

## 卸载

删除 `ModLoader\mods\CasualtiesTogether` 文件夹。

## 常见问题

- “无法写入文件”——请关闭游戏；如果仍然失败，请以管理员身份运行安装程序。
- Windows SmartScreen——点击“更多信息”→“仍要运行”（文件未签名）。
- 提示缺少 `vcruntime140.dll`——请安装 Microsoft Visual C++ Redistributable (x64)。
- 日志：`ModLoader\log.txt`。

从源码构建和编写模组请参阅英文 README。

## 许可证

LGPL-2.1。部分逻辑移植自 [casualties-together](https://github.com/creaturefeaturelarry/casualties-together)（LGPL-2.1）。

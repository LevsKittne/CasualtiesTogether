pub struct Text {
    pub not_found: &'static str,
    pub write_failed: &'static str,
    pub ask_loader: &'static str,
    pub loader_done: &'static str,
    pub ask_mod: &'static str,
    pub no_loader: &'static str,
    pub mod_done: &'static str,
    pub installed: &'static str,
    pub updated: &'static str,
}

#[link(name = "kernel32")]
unsafe extern "system" {
    fn GetUserDefaultUILanguage() -> u16;
}

pub fn text() -> &'static Text {
    let primary = unsafe { GetUserDefaultUILanguage() } & 0x3ff;
    match primary {
        0x19 => &RU,
        0x22 => &UK,
        0x04 => &ZH,
        0x0a => &ES,
        0x16 => &PT,
        0x07 => &DE,
        0x0c => &FR,
        0x11 => &JA,
        0x12 => &KO,
        0x1f => &TR,
        0x15 => &PL,
        _ => &EN,
    }
}

pub fn fill(template: &str, args: &[&str]) -> String {
    let mut out = template.to_owned();
    for (i, a) in args.iter().enumerate() {
        out = out.replace(&format!("{{{i}}}"), a);
    }
    out
}

const EN: Text = Text {
    not_found: "Game not found.\n\nPut the installer into the game folder (next to CasualtiesUnknown.exe) and run it again.",
    write_failed: "Could not write files:\n{0}\n\nClose the game and try again. If that does not help, run the installer as administrator.",
    ask_loader: "The installer is not in the game folder.\n\nFound the game:\n{0}\n\nInstall there?",
    loader_done: "ModLoader {0}.\n\nGame folder:\n{1}\n\nPut mods into ModLoader\\mods.\nThe installer will now delete itself.",
    ask_mod: "Found the game:\n{0}\n\nInstall the mod there?",
    no_loader: "ModLoader is not installed in:\n{0}\n\nInstall ModLoader first (CUModLoader-Setup.exe), then this mod.",
    mod_done: "Casualties: Together {0}.\n\n{1}\n\nStart the game. The installer will now delete itself.",
    installed: "installed",
    updated: "updated",
};

const RU: Text = Text {
    not_found: "Игра не найдена.\n\nПоложите установщик в корневую папку игры (рядом с CasualtiesUnknown.exe) и запустите снова.",
    write_failed: "Не удалось записать файлы:\n{0}\n\nЗакройте игру и попробуйте снова. Если не помогло — запустите установщик от имени администратора.",
    ask_loader: "Установщик лежит не в папке игры.\n\nНайдена игра:\n{0}\n\nУстановить туда?",
    loader_done: "ModLoader {0}.\n\nПапка игры:\n{1}\n\nМоды кладите в ModLoader\\mods.\nУстановщик сейчас удалится.",
    ask_mod: "Найдена игра:\n{0}\n\nУстановить мод туда?",
    no_loader: "В папке игры нет ModLoader:\n{0}\n\nСначала установите ModLoader (CUModLoader-Setup.exe), потом этот мод.",
    mod_done: "Мод Casualties: Together {0}.\n\n{1}\n\nЗапускайте игру. Установщик сейчас удалится.",
    installed: "установлен",
    updated: "обновлён",
};

const UK: Text = Text {
    not_found: "Гру не знайдено.\n\nПокладіть інсталятор у папку гри (поруч із CasualtiesUnknown.exe) і запустіть знову.",
    write_failed: "Не вдалося записати файли:\n{0}\n\nЗакрийте гру і спробуйте ще раз. Якщо не допомогло — запустіть інсталятор від імені адміністратора.",
    ask_loader: "Інсталятор лежить не в папці гри.\n\nЗнайдено гру:\n{0}\n\nВстановити туди?",
    loader_done: "ModLoader {0}.\n\nПапка гри:\n{1}\n\nМоди кладіть у ModLoader\\mods.\nІнсталятор зараз видалиться.",
    ask_mod: "Знайдено гру:\n{0}\n\nВстановити мод туди?",
    no_loader: "У папці гри немає ModLoader:\n{0}\n\nСпочатку встановіть ModLoader (CUModLoader-Setup.exe), потім цей мод.",
    mod_done: "Мод Casualties: Together {0}.\n\n{1}\n\nЗапускайте гру. Інсталятор зараз видалиться.",
    installed: "встановлено",
    updated: "оновлено",
};

const ZH: Text = Text {
    not_found: "未找到游戏。\n\n请将安装程序放入游戏文件夹（与 CasualtiesUnknown.exe 同目录）后重新运行。",
    write_failed: "无法写入文件：\n{0}\n\n请关闭游戏后重试。如果仍然失败，请以管理员身份运行安装程序。",
    ask_loader: "安装程序不在游戏文件夹中。\n\n找到游戏：\n{0}\n\n是否安装到此处？",
    loader_done: "ModLoader {0}。\n\n游戏文件夹：\n{1}\n\n请将模组放入 ModLoader\\mods。\n安装程序将自动删除。",
    ask_mod: "找到游戏：\n{0}\n\n是否将模组安装到此处？",
    no_loader: "以下文件夹中未安装 ModLoader：\n{0}\n\n请先安装 ModLoader（CUModLoader-Setup.exe），再安装此模组。",
    mod_done: "Casualties: Together {0}。\n\n{1}\n\n现在可以启动游戏。安装程序将自动删除。",
    installed: "已安装",
    updated: "已更新",
};

const ES: Text = Text {
    not_found: "No se encontró el juego.\n\nColoca el instalador en la carpeta del juego (junto a CasualtiesUnknown.exe) y ejecútalo de nuevo.",
    write_failed: "No se pudieron escribir los archivos:\n{0}\n\nCierra el juego e inténtalo de nuevo. Si no funciona, ejecuta el instalador como administrador.",
    ask_loader: "El instalador no está en la carpeta del juego.\n\nJuego encontrado:\n{0}\n\n¿Instalar ahí?",
    loader_done: "ModLoader {0}.\n\nCarpeta del juego:\n{1}\n\nPon los mods en ModLoader\\mods.\nEl instalador se eliminará ahora.",
    ask_mod: "Juego encontrado:\n{0}\n\n¿Instalar el mod ahí?",
    no_loader: "ModLoader no está instalado en:\n{0}\n\nInstala primero ModLoader (CUModLoader-Setup.exe) y después este mod.",
    mod_done: "Casualties: Together {0}.\n\n{1}\n\nInicia el juego. El instalador se eliminará ahora.",
    installed: "instalado",
    updated: "actualizado",
};

const PT: Text = Text {
    not_found: "Jogo não encontrado.\n\nColoque o instalador na pasta do jogo (ao lado de CasualtiesUnknown.exe) e execute-o novamente.",
    write_failed: "Não foi possível gravar os arquivos:\n{0}\n\nFeche o jogo e tente novamente. Se não resolver, execute o instalador como administrador.",
    ask_loader: "O instalador não está na pasta do jogo.\n\nJogo encontrado:\n{0}\n\nInstalar lá?",
    loader_done: "ModLoader {0}.\n\nPasta do jogo:\n{1}\n\nColoque os mods em ModLoader\\mods.\nO instalador será excluído agora.",
    ask_mod: "Jogo encontrado:\n{0}\n\nInstalar o mod lá?",
    no_loader: "O ModLoader não está instalado em:\n{0}\n\nInstale primeiro o ModLoader (CUModLoader-Setup.exe) e depois este mod.",
    mod_done: "Casualties: Together {0}.\n\n{1}\n\nInicie o jogo. O instalador será excluído agora.",
    installed: "instalado",
    updated: "atualizado",
};

const DE: Text = Text {
    not_found: "Spiel nicht gefunden.\n\nLege den Installer in den Spielordner (neben CasualtiesUnknown.exe) und starte ihn erneut.",
    write_failed: "Dateien konnten nicht geschrieben werden:\n{0}\n\nSchließe das Spiel und versuche es erneut. Falls das nicht hilft, starte den Installer als Administrator.",
    ask_loader: "Der Installer liegt nicht im Spielordner.\n\nGefundenes Spiel:\n{0}\n\nDorthin installieren?",
    loader_done: "ModLoader {0}.\n\nSpielordner:\n{1}\n\nMods kommen in ModLoader\\mods.\nDer Installer löscht sich jetzt selbst.",
    ask_mod: "Gefundenes Spiel:\n{0}\n\nMod dorthin installieren?",
    no_loader: "ModLoader ist nicht installiert in:\n{0}\n\nInstalliere zuerst ModLoader (CUModLoader-Setup.exe), dann diesen Mod.",
    mod_done: "Casualties: Together {0}.\n\n{1}\n\nStarte das Spiel. Der Installer löscht sich jetzt selbst.",
    installed: "installiert",
    updated: "aktualisiert",
};

const FR: Text = Text {
    not_found: "Jeu introuvable.\n\nPlacez l'installateur dans le dossier du jeu (à côté de CasualtiesUnknown.exe) et relancez-le.",
    write_failed: "Impossible d'écrire les fichiers :\n{0}\n\nFermez le jeu et réessayez. Si cela ne suffit pas, lancez l'installateur en tant qu'administrateur.",
    ask_loader: "L'installateur n'est pas dans le dossier du jeu.\n\nJeu trouvé :\n{0}\n\nInstaller ici ?",
    loader_done: "ModLoader {0}.\n\nDossier du jeu :\n{1}\n\nPlacez les mods dans ModLoader\\mods.\nL'installateur va maintenant se supprimer.",
    ask_mod: "Jeu trouvé :\n{0}\n\nInstaller le mod ici ?",
    no_loader: "ModLoader n'est pas installé dans :\n{0}\n\nInstallez d'abord ModLoader (CUModLoader-Setup.exe), puis ce mod.",
    mod_done: "Casualties: Together {0}.\n\n{1}\n\nLancez le jeu. L'installateur va maintenant se supprimer.",
    installed: "installé",
    updated: "mis à jour",
};

const JA: Text = Text {
    not_found: "ゲームが見つかりません。\n\nインストーラーをゲームフォルダー（CasualtiesUnknown.exe と同じ場所）に置いて、もう一度実行してください。",
    write_failed: "ファイルを書き込めませんでした：\n{0}\n\nゲームを終了してからもう一度お試しください。解決しない場合は、管理者としてインストーラーを実行してください。",
    ask_loader: "インストーラーがゲームフォルダーにありません。\n\n見つかったゲーム：\n{0}\n\nここにインストールしますか？",
    loader_done: "ModLoader を{0}。\n\nゲームフォルダー：\n{1}\n\nMOD は ModLoader\\mods に置いてください。\nインストーラーは自動的に削除されます。",
    ask_mod: "見つかったゲーム：\n{0}\n\nここに MOD をインストールしますか？",
    no_loader: "次のフォルダーに ModLoader がインストールされていません：\n{0}\n\n先に ModLoader（CUModLoader-Setup.exe）をインストールしてから、この MOD をインストールしてください。",
    mod_done: "Casualties: Together を{0}。\n\n{1}\n\nゲームを起動してください。インストーラーは自動的に削除されます。",
    installed: "インストールしました",
    updated: "更新しました",
};

const KO: Text = Text {
    not_found: "게임을 찾을 수 없습니다.\n\n설치 프로그램을 게임 폴더(CasualtiesUnknown.exe가 있는 곳)에 넣고 다시 실행하세요.",
    write_failed: "파일을 쓸 수 없습니다:\n{0}\n\n게임을 종료하고 다시 시도하세요. 그래도 안 되면 관리자 권한으로 설치 프로그램을 실행하세요.",
    ask_loader: "설치 프로그램이 게임 폴더에 있지 않습니다.\n\n찾은 게임:\n{0}\n\n여기에 설치할까요?",
    loader_done: "ModLoader가 {0}.\n\n게임 폴더:\n{1}\n\n모드는 ModLoader\\mods에 넣으세요.\n설치 프로그램은 이제 자동으로 삭제됩니다.",
    ask_mod: "찾은 게임:\n{0}\n\n여기에 모드를 설치할까요?",
    no_loader: "다음 폴더에 ModLoader가 설치되어 있지 않습니다:\n{0}\n\n먼저 ModLoader(CUModLoader-Setup.exe)를 설치한 다음 이 모드를 설치하세요.",
    mod_done: "Casualties: Together가 {0}.\n\n{1}\n\n게임을 시작하세요. 설치 프로그램은 이제 자동으로 삭제됩니다.",
    installed: "설치되었습니다",
    updated: "업데이트되었습니다",
};

const TR: Text = Text {
    not_found: "Oyun bulunamadı.\n\nYükleyiciyi oyun klasörüne (CasualtiesUnknown.exe dosyasının yanına) koyun ve tekrar çalıştırın.",
    write_failed: "Dosyalar yazılamadı:\n{0}\n\nOyunu kapatıp tekrar deneyin. Sorun devam ederse yükleyiciyi yönetici olarak çalıştırın.",
    ask_loader: "Yükleyici oyun klasöründe değil.\n\nBulunan oyun:\n{0}\n\nBuraya kurulsun mu?",
    loader_done: "ModLoader {0}.\n\nOyun klasörü:\n{1}\n\nModları ModLoader\\mods klasörüne koyun.\nYükleyici şimdi kendini silecek.",
    ask_mod: "Bulunan oyun:\n{0}\n\nMod buraya kurulsun mu?",
    no_loader: "Şu klasörde ModLoader kurulu değil:\n{0}\n\nÖnce ModLoader'ı (CUModLoader-Setup.exe), sonra bu modu kurun.",
    mod_done: "Casualties: Together {0}.\n\n{1}\n\nOyunu başlatın. Yükleyici şimdi kendini silecek.",
    installed: "kuruldu",
    updated: "güncellendi",
};

const PL: Text = Text {
    not_found: "Nie znaleziono gry.\n\nUmieść instalator w folderze gry (obok CasualtiesUnknown.exe) i uruchom go ponownie.",
    write_failed: "Nie udało się zapisać plików:\n{0}\n\nZamknij grę i spróbuj ponownie. Jeśli to nie pomoże, uruchom instalator jako administrator.",
    ask_loader: "Instalator nie znajduje się w folderze gry.\n\nZnaleziono grę:\n{0}\n\nZainstalować tutaj?",
    loader_done: "ModLoader {0}.\n\nFolder gry:\n{1}\n\nMody umieszczaj w ModLoader\\mods.\nInstalator zaraz się usunie.",
    ask_mod: "Znaleziono grę:\n{0}\n\nZainstalować tam moda?",
    no_loader: "ModLoader nie jest zainstalowany w:\n{0}\n\nNajpierw zainstaluj ModLoader (CUModLoader-Setup.exe), a potem ten mod.",
    mod_done: "Casualties: Together {0}.\n\n{1}\n\nUruchom grę. Instalator zaraz się usunie.",
    installed: "zainstalowany",
    updated: "zaktualizowany",
};

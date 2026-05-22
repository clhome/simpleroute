#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Language {
    Zh,
    En,
}

impl Language {
    /// 自动检测 Windows 系统的当前 UI 语言
    pub fn detect() -> Self {
        // 使用 Windows 原生 kernel32 API 获取当前用户默认 UI 语言的 LANGID
        #[link(name = "kernel32")]
        unsafe extern "system" {
            fn GetUserDefaultUILanguage() -> u16;
        }

        let lang_id = unsafe { GetUserDefaultUILanguage() };
        // LANGID 的低 10 位是主语言 ID (Primary Language ID)
        // 在 Windows API 中，LANG_CHINESE = 0x04
        if (lang_id & 0x3FF) == 0x04 {
            Language::Zh
        } else {
            Language::En
        }
    }

    /// 根据当前语言，返回对应的中文或英文翻译
    pub fn t<'a>(&self, zh: &'a str, en: &'a str) -> &'a str {
        match self {
            Language::Zh => zh,
            Language::En => en,
        }
    }
}

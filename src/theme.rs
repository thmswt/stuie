use ratatui::style::Color;

pub struct AppTheme {
    pub name: &'static str,
    pub description: &'static str,

    // Borders
    pub border_active: Color,
    pub border_inactive: Color,

    // Text hierarchy
    pub text_primary: Color,
    pub text_secondary: Color,
    pub text_muted: Color,

    // Accent
    pub accent: Color,

    // File list
    pub file_color: Color,
    pub folder_color: Color,

    // Selection / highlight
    pub highlight_fg: Color,
    pub highlight_bg: Color,

    // Editing (inverse for text input fields)
    pub editing_fg: Color,
    pub editing_bg: Color,

    // Tabs
    pub tab_active_fg: Color,
    pub tab_active_bg: Color,

    // Modal
    pub modal_border: Color,

    // Status
    pub error: Color,
    pub success: Color,
    pub warning: Color,
}

pub const BUILTIN_THEMES: &[AppTheme] = &[
    // Default — matches the original hardcoded colors
    AppTheme {
        name: "default",
        description: "Classic stuie look",
        border_active: Color::Yellow,
        border_inactive: Color::Reset,
        text_primary: Color::White,
        text_secondary: Color::Gray,
        text_muted: Color::DarkGray,
        accent: Color::Cyan,
        file_color: Color::White,
        folder_color: Color::Cyan,
        highlight_fg: Color::Yellow,
        highlight_bg: Color::DarkGray,
        editing_fg: Color::Black,
        editing_bg: Color::Yellow,
        tab_active_fg: Color::Black,
        tab_active_bg: Color::Cyan,
        modal_border: Color::Cyan,
        error: Color::Red,
        success: Color::Green,
        warning: Color::Yellow,
    },
    // Dracula
    AppTheme {
        name: "dracula",
        description: "Dark purple Dracula palette",
        border_active: Color::Rgb(189, 147, 249),
        border_inactive: Color::Rgb(68, 71, 90),
        text_primary: Color::Rgb(248, 248, 242),
        text_secondary: Color::Rgb(98, 114, 164),
        text_muted: Color::Rgb(68, 71, 90),
        accent: Color::Rgb(139, 233, 253),
        file_color: Color::Rgb(248, 248, 242),
        folder_color: Color::Rgb(139, 233, 253),
        highlight_fg: Color::Rgb(80, 250, 123),
        highlight_bg: Color::Rgb(68, 71, 90),
        editing_fg: Color::Rgb(40, 42, 54),
        editing_bg: Color::Rgb(255, 121, 198),
        tab_active_fg: Color::Rgb(40, 42, 54),
        tab_active_bg: Color::Rgb(189, 147, 249),
        modal_border: Color::Rgb(189, 147, 249),
        error: Color::Rgb(255, 85, 85),
        success: Color::Rgb(80, 250, 123),
        warning: Color::Rgb(241, 250, 140),
    },
    // Light
    AppTheme {
        name: "light",
        description: "Clean light theme for bright terminals",
        border_active: Color::Blue,
        border_inactive: Color::Rgb(200, 200, 200),
        text_primary: Color::Rgb(30, 30, 30),
        text_secondary: Color::Rgb(100, 100, 100),
        text_muted: Color::Rgb(170, 170, 170),
        accent: Color::Rgb(0, 120, 180),
        file_color: Color::Rgb(30, 30, 30),
        folder_color: Color::Rgb(0, 120, 180),
        highlight_fg: Color::White,
        highlight_bg: Color::Rgb(0, 100, 200),
        editing_fg: Color::White,
        editing_bg: Color::Rgb(0, 100, 200),
        tab_active_fg: Color::White,
        tab_active_bg: Color::Blue,
        modal_border: Color::Blue,
        error: Color::Rgb(200, 30, 30),
        success: Color::Rgb(0, 140, 50),
        warning: Color::Rgb(200, 150, 0),
    },
    // Retro Green (CRT / Matrix)
    AppTheme {
        name: "retro",
        description: "Green phosphor terminal",
        border_active: Color::Rgb(0, 255, 65),
        border_inactive: Color::Rgb(0, 80, 20),
        text_primary: Color::Rgb(0, 210, 50),
        text_secondary: Color::Rgb(0, 150, 40),
        text_muted: Color::Rgb(0, 80, 20),
        accent: Color::Rgb(0, 255, 65),
        file_color: Color::Rgb(0, 210, 50),
        folder_color: Color::Rgb(0, 255, 65),
        highlight_fg: Color::Black,
        highlight_bg: Color::Rgb(0, 210, 50),
        editing_fg: Color::Black,
        editing_bg: Color::Rgb(0, 255, 65),
        tab_active_fg: Color::Black,
        tab_active_bg: Color::Rgb(0, 210, 50),
        modal_border: Color::Rgb(0, 210, 50),
        error: Color::Rgb(255, 80, 80),
        success: Color::Rgb(0, 255, 65),
        warning: Color::Rgb(200, 200, 0),
    },
    // Ocean
    AppTheme {
        name: "ocean",
        description: "Cool blue oceanic palette",
        border_active: Color::Rgb(100, 200, 255),
        border_inactive: Color::Rgb(50, 70, 90),
        text_primary: Color::Rgb(200, 220, 240),
        text_secondary: Color::Rgb(120, 150, 180),
        text_muted: Color::Rgb(60, 80, 100),
        accent: Color::Rgb(100, 200, 255),
        file_color: Color::Rgb(200, 220, 240),
        folder_color: Color::Rgb(100, 200, 255),
        highlight_fg: Color::Rgb(255, 220, 100),
        highlight_bg: Color::Rgb(30, 60, 90),
        editing_fg: Color::Rgb(10, 20, 40),
        editing_bg: Color::Rgb(100, 200, 255),
        tab_active_fg: Color::Rgb(10, 20, 40),
        tab_active_bg: Color::Rgb(100, 200, 255),
        modal_border: Color::Rgb(100, 200, 255),
        error: Color::Rgb(255, 100, 100),
        success: Color::Rgb(100, 220, 150),
        warning: Color::Rgb(255, 200, 80),
    },
    // Solarized Dark
    AppTheme {
        name: "solarized",
        description: "Warm solarized dark palette",
        border_active: Color::Rgb(181, 137, 0),
        border_inactive: Color::Rgb(88, 110, 117),
        text_primary: Color::Rgb(147, 161, 161),
        text_secondary: Color::Rgb(101, 123, 131),
        text_muted: Color::Rgb(88, 110, 117),
        accent: Color::Rgb(38, 139, 210),
        file_color: Color::Rgb(147, 161, 161),
        folder_color: Color::Rgb(38, 139, 210),
        highlight_fg: Color::Rgb(253, 246, 227),
        highlight_bg: Color::Rgb(7, 54, 66),
        editing_fg: Color::Rgb(0, 43, 54),
        editing_bg: Color::Rgb(181, 137, 0),
        tab_active_fg: Color::Rgb(0, 43, 54),
        tab_active_bg: Color::Rgb(38, 139, 210),
        modal_border: Color::Rgb(38, 139, 210),
        error: Color::Rgb(220, 50, 47),
        success: Color::Rgb(133, 153, 0),
        warning: Color::Rgb(203, 75, 22),
    },
    // Nord
    AppTheme {
        name: "nord",
        description: "Arctic, north-bluish clean theme",
        border_active: Color::Rgb(136, 192, 208),
        border_inactive: Color::Rgb(76, 86, 106),
        text_primary: Color::Rgb(229, 233, 240),
        text_secondary: Color::Rgb(216, 222, 233),
        text_muted: Color::Rgb(76, 86, 106),
        accent: Color::Rgb(136, 192, 208),
        file_color: Color::Rgb(229, 233, 240),
        folder_color: Color::Rgb(136, 192, 208),
        highlight_fg: Color::Rgb(236, 239, 244),
        highlight_bg: Color::Rgb(67, 76, 94),
        editing_fg: Color::Rgb(46, 52, 64),
        editing_bg: Color::Rgb(136, 192, 208),
        tab_active_fg: Color::Rgb(46, 52, 64),
        tab_active_bg: Color::Rgb(136, 192, 208),
        modal_border: Color::Rgb(129, 161, 193),
        error: Color::Rgb(191, 97, 106),
        success: Color::Rgb(163, 190, 140),
        warning: Color::Rgb(235, 203, 139),
    },
];

pub fn get_theme(name: &str) -> &'static AppTheme {
    BUILTIN_THEMES
        .iter()
        .find(|t| t.name == name)
        .unwrap_or(&BUILTIN_THEMES[0])
}

pub fn theme_index(name: &str) -> usize {
    BUILTIN_THEMES
        .iter()
        .position(|t| t.name == name)
        .unwrap_or(0)
}

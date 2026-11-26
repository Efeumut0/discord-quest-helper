use tray_icon::menu::Menu;
use tray_icon::Icon;
use tray_icon::TrayIcon;
use tray_icon::TrayIconBuilder;

pub fn create_tray_icon(tray_menu: Menu, title: &str) -> Option<TrayIcon> {
    let icon = Icon::from_resource(1_u16, None).ok()?;

    TrayIconBuilder::new()
        .with_menu(Box::new(tray_menu))
        .with_tooltip(title)
        .with_icon(icon)
        .build()
        .ok()
}

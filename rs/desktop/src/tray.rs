//! The tray icon and its menu (Open Hearth, Ring me on this computer, Quit).
//! Linux: the StatusNotifierItem protocol over D-Bus (ksni, no GTK and no appindicator library; KDE, GNOME with the AppIndicator extension, Hyprland bars, Oxide...).
//! Windows: the notification area (tray-icon crate). Without a tray host the app simply has no tray icon.
use crate::app::{App, Event, TRAY_PNG};
use std::sync::Arc;

/// The tray's pixels as ARGB32 (network byte order), what the StatusNotifierItem protocol wants.
pub fn argb(png: &[u8]) -> Option<(Vec<u8>, i32, i32)> {
    let (rgba, w, h) = crate::ui::decode_png_rgba(png)?;
    Some((rgba.chunks(4).flat_map(|p| [p[3], p[0], p[1], p[2]]).collect(), w as i32, h as i32))
}

pub struct TrayHandle {
    #[cfg(target_os = "linux")] handle: ksni::Handle<linux::HearthTray>,
    #[cfg(target_os = "linux")] rt: tokio::runtime::Handle,
    #[cfg(windows)] ring: tray_icon::menu::CheckMenuItem,
    #[cfg(windows)] _icon: tray_icon::TrayIcon,
}

#[cfg(target_os = "linux")]
mod linux {
    use super::*;
    pub struct HearthTray { pub app: Arc<App>, pub ring: bool, pub icon: Option<(Vec<u8>, i32, i32)> }
    impl ksni::Tray for HearthTray {
        fn id(&self) -> String { "hearth".into() }
        fn title(&self) -> String { "Hearth".into() }
        fn icon_pixmap(&self) -> Vec<ksni::Icon> { self.icon.iter().map(|(d, w, h)| ksni::Icon { width: *w, height: *h, data: d.clone() }).collect() }
        fn tool_tip(&self) -> ksni::ToolTip { ksni::ToolTip { title: "Hearth".into(), ..Default::default() } }
        fn activate(&mut self, _x: i32, _y: i32) { self.app.emit(Event::ShowMain); }
        fn menu(&self) -> Vec<ksni::MenuItem<Self>> {
            use ksni::menu::*;
            vec![
                StandardItem { label: "Open Hearth".into(), activate: Box::new(|t: &mut Self| t.app.emit(Event::ShowMain)), ..Default::default() }.into(),
                CheckmarkItem { label: "Ring me on this computer".into(), checked: self.ring, activate: Box::new(|t: &mut Self| { let on = !t.app.call_me(); t.app.set_call_me(on); }), ..Default::default() }.into(),
                MenuItem::Separator,
                StandardItem { label: "Quit".into(), activate: Box::new(|t: &mut Self| t.app.quit()), ..Default::default() }.into(),
            ]
        }
    }
}

/// Puts the icon in the tray. Err = no tray host on this desktop (the app works without it).
#[cfg(target_os = "linux")]
pub async fn start(app: Arc<App>) -> Result<TrayHandle, String> {
    use ksni::TrayMethods;
    let rt = app.rt.clone();
    let t = linux::HearthTray { ring: app.call_me(), icon: argb(TRAY_PNG), app };
    let handle = t.spawn().await.map_err(|e| e.to_string())?;
    Ok(TrayHandle { handle, rt })
}
#[cfg(target_os = "linux")]
impl TrayHandle {
    pub fn set_ring(&self, on: bool) { let h = self.handle.clone(); self.rt.spawn(async move { h.update(|t| t.ring = on).await; }); }
    pub fn set_icon(&self, png: &[u8]) { if let Some(i) = argb(png) { let h = self.handle.clone(); self.rt.spawn(async move { h.update(|t| t.icon = Some(i)).await; }); } }
}

#[cfg(windows)]
pub async fn start(app: Arc<App>) -> Result<TrayHandle, String> {
    use tray_icon::menu::{CheckMenuItem, Menu, MenuEvent, MenuItem, PredefinedMenuItem};
    use tray_icon::{Icon, MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
    let (rgba, w, h) = crate::ui::decode_png_rgba(TRAY_PNG).ok_or("tray icon")?;
    let open = MenuItem::new("Open Hearth", true, None);
    let ring = CheckMenuItem::new("Ring me on this computer", true, app.call_me(), None);
    let quit = MenuItem::new("Quit", true, None);
    let menu = Menu::new();
    menu.append_items(&[&open, &ring, &PredefinedMenuItem::separator(), &quit]).map_err(|e| e.to_string())?;
    let icon = TrayIconBuilder::new().with_icon(Icon::from_rgba(rgba, w, h).map_err(|e| e.to_string())?).with_tooltip("Hearth").with_menu(Box::new(menu)).with_menu_on_left_click(false).build().map_err(|e| e.to_string())?;
    let (oid, rid, qid) = (open.id().clone(), ring.id().clone(), quit.id().clone());
    let a = app.clone();
    MenuEvent::set_event_handler(Some(move |e: MenuEvent| { if e.id == oid { a.emit(Event::ShowMain) } else if e.id == rid { let on = !a.call_me(); a.set_call_me(on) } else if e.id == qid { a.quit() } }));
    let a = app.clone();
    TrayIconEvent::set_event_handler(Some(move |e: TrayIconEvent| { if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = e { a.emit(Event::ShowMain) } }));
    Ok(TrayHandle { ring, _icon: icon })
}
#[cfg(windows)]
impl TrayHandle {
    pub fn set_ring(&self, on: bool) { self.ring.set_checked(on); }
    pub fn set_icon(&self, png: &[u8]) { if let Some((rgba, w, h)) = crate::ui::decode_png_rgba(png) { if let Ok(i) = tray_icon::Icon::from_rgba(rgba, w, h) { let _ = self._icon.set_icon(Some(i)); } } }
}

#[cfg(not(any(target_os = "linux", windows)))]
pub async fn start(_app: Arc<App>) -> Result<TrayHandle, String> { Err("no tray on this system".into()) }
#[cfg(not(any(target_os = "linux", windows)))]
impl TrayHandle { pub fn set_ring(&self, _on: bool) {} pub fn set_icon(&self, _png: &[u8]) {} }

#[cfg(test)]
mod tests {
    #[test] fn tray_png_is_argb() { let (d, w, h) = super::argb(super::TRAY_PNG).unwrap(); assert_eq!(d.len() as i32, w * h * 4); assert!(w > 0 && h > 0); }
}

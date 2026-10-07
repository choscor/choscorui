# Desktop integration (macOS)

| Feature | API | Behavior | Requirements |
| --- | --- | --- | --- |
| Activation | `Application::take_app_events` → `AppEvent::Activated` / `Deactivated` | Sent when the app becomes or stops being frontmost. | None |
| Reopen | `AppEvent::Reopen { has_visible_windows }` | Sent when the Dock icon is clicked while the app is running. AppKit takes no default action, so the app decides what to do. When `has_visible_windows` is false, the app should show its main window. | None |
| Open documents | `AppEvent::OpenFiles(paths)` | Sent for Finder double-clicks, drops on the Dock icon, and `open -a`. Paths are delivered as they arrive, including at launch. | A bundle `Info.plist` must declare `CFBundleDocumentTypes` for Finder to route files to the app. |
| Custom URL schemes | `AppEvent::OpenUrls(urls)` | Sent for non-file URLs. | `CFBundleURLTypes` in `Info.plist` |
| Open URL | `open_url(url)`, `url_handler(url)` | Opens the URL with the default handler (browser, mail client, Finder). `url_handler` only queries which app would open it. Errors: `InvalidUrl` for unparsable text or a URL with no scheme; `NoHandler` when no app handles the scheme. | Works in the App Sandbox. |
| Status item | `StatusItem::new(title)`, `set_title`, `set_tooltip`, `set_menu(ContextMenu)`, `set_visible` | Adds a menu-bar extra. Menu choices queue in `item.menu().take_activations()`. Dropping the item removes it from the menu bar. | None. Pair it with `LSUIElement` for apps that live only in the menu bar. |
| Notifications | `request_notification_permission()`, `notification_permission()`, `post_notification(id, title, body)` | Asks for alert, sound, and badge permission; the answer arrives asynchronously. Posting a notification with an existing identifier replaces it. Notifications show while the app is frontmost too. Clicking one yields `AppEvent::NotificationActivated { identifier, action }`. | The app must run from a bundle with a `CFBundleIdentifier`; otherwise the call returns `RequiresAppBundle`. If the user denied permission, the call returns `PermissionDenied`. Delivery and click-through are manual checks. |

Windows and Linux backends are not shipped. Their equivalents (tray icons through StatusNotifierItem or the Windows shell, notifications through portals or toasts) are not covered by this contract.

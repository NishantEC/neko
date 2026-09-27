//! Three visible setup steps; permission asks remain substates, not extra screens.
use super::*;
use neko_protocol::workbench::{Command, Snapshot, Workspace};

pub(super) fn evidence() -> bool {
    std::env::var_os("NEKO_SHOW_SETUP").is_some()
}
fn evidence_step(value: Option<&str>) -> SetupStep {
    value
        .and_then(|s| s.parse::<usize>().ok())
        .filter(|i| (1..=3).contains(i))
        .map(|i| SetupStep::ALL[i - 1])
        .unwrap_or(SetupStep::Welcome)
}
pub(super) struct Setup {
    pub step: SetupStep,
    pub busy: bool,
    pub error: Option<String>,
    pub notice: Option<String>,
    clipboard_ready: bool,
    snapshot: Snapshot,
    pub(super) workspace: Option<String>,
}
impl Setup {
    pub fn new(_cx: &mut Context<OnboardingRoot>) -> Self {
        Self {
            step: if evidence() {
                evidence_step(std::env::var("NEKO_SETUP_STEP").ok().as_deref())
            } else {
                SetupStep::Welcome
            },
            busy: false,
            clipboard_ready: false,
            error: None,
            notice: None,
            snapshot: Snapshot::default(),
            workspace: None,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum SetupStep {
    Welcome,
    Mac,
    Workspace,
}
impl SetupStep {
    pub(super) const ALL: [Self; 3] = [Self::Welcome, Self::Mac, Self::Workspace];
    pub(super) fn index(self) -> usize {
        Self::ALL.iter().position(|s| *s == self).unwrap()
    }
    pub(super) fn next(self) -> Self {
        Self::ALL[(self.index() + 1).min(Self::ALL.len() - 1)]
    }
    pub(super) fn back(self) -> Self {
        Self::ALL[self.index().saturating_sub(1)]
    }
}
fn begin_clipboard_change(busy: &mut bool, ready: bool) -> bool {
    if *busy || !ready {
        return false;
    }
    *busy = true;
    true
}
fn recording_after_navigation(step: SetupStep, recording: RecordingState) -> RecordingState {
    if step == SetupStep::Mac {
        recording
    } else {
        RecordingState::Idle
    }
}

enum FolderChoice {
    Existing(String),
    New(Workspace),
}

fn folder_choice(path: &std::path::Path, workspaces: &[Workspace]) -> Result<FolderChoice, String> {
    let directory = path
        .canonicalize()
        .map_err(|_| "That folder is no longer available".to_owned())?;
    if !directory.is_dir() {
        return Err("Choose a folder, not a file".into());
    }
    let repository = directory
        .to_str()
        .ok_or("This folder path cannot be read")?
        .to_owned();
    if let Some(existing) = workspaces.iter().find(|w| w.repository == repository) {
        return Ok(FolderChoice::Existing(existing.id.clone()));
    }
    let name = directory
        .file_name()
        .and_then(|part| part.to_str())
        .filter(|name| !name.is_empty())
        .ok_or("Choose a project folder, not a filesystem root")?
        .to_owned();
    Ok(FolderChoice::New(Workspace {
        id: String::new(),
        name,
        repository,
        instructions: String::new(),
        away_enabled: false,
    }))
}

impl OnboardingRoot {
    fn setup_select_folder(&mut self, path: std::path::PathBuf, cx: &mut Context<Self>) {
        match folder_choice(&path, &self.setup.snapshot.workspaces) {
            Ok(FolderChoice::Existing(id)) => {
                self.setup.workspace = Some(id);
                self.setup.error = None;
                self.setup.notice = Some("Workspace ready. Open Neko to start there.".into());
                cx.notify();
            }
            Ok(FolderChoice::New(workspace)) => {
                self.setup_request(Command::SaveWorkspace { workspace }, cx);
            }
            Err(error) => {
                self.setup.error = Some(error);
                cx.notify();
            }
        }
    }

    fn setup_choose_folder(&mut self, cx: &mut Context<Self>) {
        if self.setup.busy || evidence() {
            return;
        }
        self.setup.busy = true;
        let receiver = cx.prompt_for_paths(gpui::PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: Some("Choose workspace".into()),
        });
        cx.spawn(async move |this, cx| {
            let result = receiver.await;
            let _ = this.update(cx, |root, cx| {
                root.setup.busy = false;
                match result {
                    Ok(Ok(Some(paths))) => {
                        if let Some(path) = paths.into_iter().next() {
                            root.setup_select_folder(path, cx);
                        }
                    }
                    Ok(Ok(None)) => {}
                    Ok(Err(error)) => root.setup.error = Some(error.to_string()),
                    Err(_) => root.setup.error = Some("Folder chooser closed unexpectedly".into()),
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    fn setup_workspace_view(&self, cx: &mut Context<Self>) -> gpui::Div {
        let mut body = div().flex().flex_col().gap(px(20.));
        if !self.setup.snapshot.workspaces.is_empty() {
            body = body.child(
                div()
                    .flex()
                    .justify_between()
                    .child("Your workspaces")
                    .child(note(format!(
                        "{} available",
                        self.setup.snapshot.workspaces.len()
                    ))),
            );
            let mut cards = div().flex().flex_wrap().gap(px(14.));
            for workspace in self.setup.snapshot.workspaces.iter().take(6) {
                let id = workspace.id.clone();
                let selected = self.setup.workspace.as_ref() == Some(&id);
                cards = cards.child(
                    div()
                        .id(format!("setup-folder-{id}"))
                        .tab_index(0)
                        .tab_stop(!self.setup.busy)
                        .w(px(300.))
                        .h(px(180.))
                        .p(px(20.))
                        .rounded(px(12.))
                        .border_1()
                        .border_color(if selected {
                            theme::active().text_primary
                        } else {
                            theme::active().border_hairline_strong
                        })
                        .bg(theme::active().surface_panel)
                        .cursor_pointer()
                        .hover(|style| style.bg(theme::active().surface_raised))
                        .on_click(cx.listener(move |root, _, _, cx| {
                            root.setup.workspace = Some(id.clone());
                            root.setup.error = None;
                            cx.notify();
                        }))
                        .flex()
                        .flex_col()
                        .justify_between()
                        .child(
                            div()
                                .flex()
                                .flex_col()
                                .gap(px(8.))
                                .child(
                                    div()
                                        .text_size(px(18.))
                                        .font_weight(gpui::FontWeight::SEMIBOLD)
                                        .child(workspace.name.clone()),
                                )
                                .child(note(workspace.repository.clone())),
                        )
                        .child(note(if selected {
                            "Selected"
                        } else {
                            "Select workspace →"
                        })),
                );
            }
            body = body.child(cards);
        } else {
            body = body.child(note(
                "Choose one folder where you work. Neko will leave its files and setup in place.",
            ));
        }
        body.child(button(
            "setup-choose-folder",
            "Choose another folder…",
            !self.setup.busy && !evidence(),
            cx,
            |root, _, cx| root.setup_choose_folder(cx),
        ))
        .child(note("Neko reads available workspace guidance when you work there. External tools still require your approval before use."))
    }

    fn setup_read_clipboard(&mut self, cx: &mut Context<Self>) {
        if self.setup.busy {
            return;
        }
        self.setup.busy = true;
        let client = self.client.clone();
        cx.spawn(async move |this, cx| {
            let result = client.request(Request::GetClipboardHistoryEnabled).await;
            let _ = this.update(cx, |root, cx| {
                root.setup.busy = false;
                if let Ok(Response::ClipboardHistoryEnabled { enabled }) = result {
                    root.flow.clipboard_enabled = enabled;
                    root.setup.clipboard_ready = true;
                    root.setup_request(Command::Snapshot, cx);
                }
                cx.notify();
            });
        })
        .detach();
    }

    pub(super) fn setup_start(&mut self, cx: &mut Context<Self>) {
        self.flow.accessibility_granted = self.accessibility.is_trusted();
        self.setup_read_clipboard(cx);
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(Duration::from_secs(2)).await;
                if this
                    .update(cx, |root, cx| {
                        let was_trusted = root.flow.accessibility_granted;
                        root.flow.accessibility_granted = root.accessibility.is_trusted();
                        if !was_trusted && root.flow.accessibility_granted && !evidence() {
                            root.ensure_hotkey_registered();
                        }
                        if !root.setup.busy {
                            if !root.setup.clipboard_ready {
                                root.setup_read_clipboard(cx);
                            } else {
                                root.setup_request(Command::Snapshot, cx);
                            }
                        }
                    })
                    .is_err()
                {
                    break;
                }
            }
        })
        .detach();
    }

    fn setup_request(&mut self, command: Command, cx: &mut Context<Self>) {
        if self.setup.busy {
            return;
        }
        self.setup.busy = true;
        let poll = matches!(command, Command::Snapshot);
        let saved_repository = match &command {
            Command::SaveWorkspace { workspace } => Some(workspace.repository.clone()),
            _ => None,
        };
        if !poll {
            self.setup.error = None;
        }
        let client = self.client.clone();
        cx.spawn(async move |this, cx| {
            let result = client.request(Request::Workbench(command)).await;
            let _ = this.update(cx, |root, cx| {
                root.setup.busy = false;
                match result {
                    Ok(Response::Workbench(snapshot)) => {
                        if root.setup.workspace.is_none() {
                            root.setup.workspace =
                                snapshot.workspaces.first().map(|w| w.id.clone());
                        }
                        if let Some(repository) = saved_repository.as_deref() {
                            root.setup.workspace = snapshot
                                .workspaces
                                .iter()
                                .find(|w| w.repository == repository)
                                .map(|w| w.id.clone());
                            root.setup.notice = Some(
                                "Workspace added. Neko will use what is already in that folder."
                                    .into(),
                            );
                        }
                        root.setup.snapshot = snapshot;
                    }
                    Ok(Response::Error { message }) => root.setup.error = Some(message),
                    Err(error) => root.setup.error = Some(error.to_string()),
                    _ => root.setup.error = Some("Unexpected setup response; try again.".into()),
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    pub(super) fn setup_next(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.setup.busy || matches!(self.flow.hotkey_recording, RecordingState::Recording) {
            return;
        }
        if self.setup.step == SetupStep::Workspace {
            self.close_and_persist_completion(window, cx);
            return;
        }
        self.setup.step = self.setup.step.next();
        self.flow.hotkey_recording =
            recording_after_navigation(self.setup.step, self.flow.hotkey_recording.clone());
        self.setup.error = None;
        self.setup.notice = None;
        cx.notify();
    }

    pub(super) fn render_setup(
        &mut self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        let stage = self.setup.step;
        let labels = ["Welcome", "Your Mac", "Workspace"];
        let titles = [
            "While you’re away,\nsomeone should be you.",
            "Let Neko work on your Mac",
            "Choose a workspace",
        ];
        let subtitles = [
            "One Neko to talk to. A quick panel when you need it. Your work stays yours.",
            "Each permission is optional. You can always open the full app without a shortcut.",
            "Neko works with what is already in that folder. Nothing is copied or enabled.",
        ];
        let mut progress = div().flex().gap(px(6.)).items_center();
        for (index, label) in labels.iter().enumerate() {
            let _ = label;
            progress = progress.child(div().w(px(20.)).h(px(3.)).rounded(px(2.)).bg(
                if index == stage.index() {
                    theme::active().text_primary
                } else {
                    theme::active().border_hairline_strong
                },
            ));
        }
        let mut body = div()
            .id("setup-body")
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .px(px(if stage == SetupStep::Workspace {
                76.
            } else {
                90.
            }))
            .py(px(if stage == SetupStep::Workspace {
                26.
            } else {
                28.
            }))
            .flex()
            .flex_col()
            .gap(px(18.))
            .when(stage == SetupStep::Welcome, |b| {
                b.items_center().justify_center().text_center()
            });
        if stage == SetupStep::Welcome {
            body = body.child(
                gpui::svg()
                    .path(crate::assets::icon::MARK)
                    .size(px(88.))
                    .text_color(theme::active().text_primary),
            );
        }
        if stage == SetupStep::Workspace {
            body = body.child(
                div()
                    .text_size(px(12.))
                    .font_weight(gpui::FontWeight::BOLD)
                    .text_color(gpui::rgb(0x92969d))
                    .child("WORK WHERE YOU ALREADY WORK"),
            );
        }
        body = body
            .child(
                div()
                    .text_size(px(if stage == SetupStep::Welcome {
                        48.
                    } else if stage == SetupStep::Workspace {
                        34.
                    } else {
                        30.
                    }))
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .line_height(px(if stage == SetupStep::Welcome {
                        54.
                    } else {
                        38.
                    }))
                    .child(titles[stage.index()].to_owned()),
            )
            .child(note(subtitles[stage.index()].to_owned()));
        match stage {
            SetupStep::Welcome => {
                body=body.child(div().mt(px(18.)).child(button("setup-get-started","Get started",!self.setup.busy,cx,|r,w,cx|r.setup_next(w,cx))))
                    .child(note("Start with one workspace. Add tools and responsibilities at your own pace."));
            }
            SetupStep::Mac => {
                body = body.child(button("setup-accessibility", if self.flow.accessibility_granted { "Accessibility enabled" } else { "Allow Accessibility…" }, !self.setup.busy && !self.flow.accessibility_granted && !evidence(), cx, |r,_,cx| { r.flow.step=Step::AccessibilityAsk; r.request_accessibility(cx); }))
                    .child(note("Used for the global shortcut and paste actions. macOS controls this permission."))
                    .child(button("setup-clipboard", if !self.setup.clipboard_ready { "Reading clipboard setting…" } else if self.flow.clipboard_enabled { "Turn clipboard history off" } else { "Enable clipboard history" }, self.setup.clipboard_ready && !self.setup.busy && !evidence(), cx, |r,_,cx| {
                        if !begin_clipboard_change(&mut r.setup.busy,r.setup.clipboard_ready) { return; }
                        let enabled = !r.flow.clipboard_enabled;
                        let client=r.client.clone(); cx.notify();
                        cx.spawn(async move |this,cx| { let result=client.request(Request::SetClipboardHistoryEnabled{enabled}).await; let _=this.update(cx,|r,cx| { r.setup.busy=false; match result { Ok(Response::ClipboardHistoryEnabled{enabled:actual}) if actual==enabled=>r.flow.clipboard_enabled=actual,Ok(Response::Error{message})=>r.setup.error=Some(message),Err(e)=>r.setup.error=Some(e.to_string()),Ok(_)=>r.setup.error=Some("Clipboard setting was not acknowledged. Try again.".into()) }; cx.notify(); }); }).detach();
                    }))
                    .child(note("Saved locally. Off by default; enabling starts clipboard capture."))
                    .child(button("setup-shortcut", format!("Shortcut: {} · Change", self.flow.current_combo.display().replace('⌥',"Option + ").replace('⌘',"Command + ").replace('⌃',"Control + ").replace('⇧',"Shift + ")), self.flow.accessibility_granted && !self.setup.busy && !evidence(), cx, |r,_,cx| { r.flow.step=Step::LearnHotkey; r.toggle_recording(cx); }));
                if matches!(self.flow.hotkey_recording, RecordingState::Recording) {
                    body = body.child(note(
                        "Press the new key combination. Escape cancels recording.",
                    ));
                }
                if let RecordingState::Rejected(reason) = &self.flow.hotkey_recording {
                    body = body.child(note(reason.clone()));
                }
            }
            SetupStep::Workspace => {
                body = body.child(self.setup_workspace_view(cx));
            }
        }
        if let Some(error) = &self.setup.error {
            body = body.child(note(format!("Could not complete: {error}")));
        }
        if let Some(notice) = &self.setup.notice {
            body = body.child(note(notice.clone()));
        }
        let footer = div()
            .flex()
            .justify_between()
            .items_center()
            .px(px(40.))
            .py(px(18.))
            .border_t_1()
            .border_color(theme::active().border_hairline)
            .child(button(
                "setup-back",
                "Back",
                !self.setup.busy && stage != SetupStep::Welcome,
                cx,
                |r, _, cx| {
                    r.setup.step = r.setup.step.back();
                    r.flow.hotkey_recording =
                        recording_after_navigation(r.setup.step, r.flow.hotkey_recording.clone());
                    cx.notify();
                },
            ))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(12.))
                    .when(stage != SetupStep::Workspace, |actions| {
                        actions.child(button(
                            "setup-skip",
                            "Set up later",
                            !self.setup.busy,
                            cx,
                            |r, w, cx| r.close_and_persist_completion(w, cx),
                        ))
                    })
                    .child(button(
                        "setup-next",
                        if stage == SetupStep::Workspace {
                            if self.setup.workspace.is_some() {
                                "Open Neko →".to_owned()
                            } else {
                                "I'll do this later →".to_owned()
                            }
                        } else {
                            "Continue".to_owned()
                        },
                        !self.setup.busy,
                        cx,
                        |r, w, cx| r.setup_next(w, cx),
                    )),
            );
        let header = if stage == SetupStep::Workspace {
            div()
                .h(px(54.))
                .px(px(24.))
                .flex()
                .items_center()
                .justify_between()
                .border_b_1()
                .border_color(gpui::rgb(0x303136))
                .window_control_area(gpui::WindowControlArea::Drag)
                .child(div().w(px(80.)))
                .child(
                    div()
                        .text_size(px(13.))
                        .font_weight(gpui::FontWeight::SEMIBOLD)
                        .child("Neko"),
                )
                .child(
                    div()
                        .w(px(80.))
                        .text_align(gpui::TextAlign::Right)
                        .text_size(px(12.))
                        .text_color(gpui::rgb(0x92949a))
                        .child("Setup · 3 of 3"),
                )
        } else {
            div()
                .h(px(52.))
                .flex_shrink_0()
                .flex()
                .items_center()
                .justify_center()
                .window_control_area(gpui::WindowControlArea::Drag)
                .child(progress)
        };
        div()
            .id("neko-setup")
            .tab_group()
            .key_context("Onboarding")
            .track_focus(&self.focus_handle)
            .on_key_down(cx.listener(Self::on_key_down))
            .flex()
            .flex_col()
            .size_full()
            .bg(if stage == SetupStep::Workspace {
                gpui::rgb(0x1c1d20)
            } else {
                theme::active().surface_panel
            })
            .text_color(theme::active().text_primary)
            .child(header)
            .child(body)
            .when(stage != SetupStep::Welcome, |view| view.child(footer))
            .into_any_element()
    }
}

fn note(text: impl Into<SharedString>) -> gpui::Div {
    div()
        .text_size(px(13.))
        .line_height(px(20.))
        .text_color(theme::active().text_secondary)
        .child(text.into())
}
fn button(
    id: impl Into<SharedString>,
    label: impl Into<SharedString>,
    enabled: bool,
    cx: &mut Context<OnboardingRoot>,
    action: impl Fn(&mut OnboardingRoot, &mut Window, &mut Context<OnboardingRoot>) + 'static,
) -> impl IntoElement {
    let action = Rc::new(action);
    let keyboard = action.clone();
    div()
        .id(id.into())
        .tab_index(0)
        .tab_stop(enabled)
        .px(px(14.))
        .py(px(10.))
        .rounded(px(8.))
        .border_1()
        .border_color(theme::active().border_hairline)
        .bg(theme::active().surface_input)
        .text_size(px(13.))
        .text_color(if enabled {
            theme::active().text_primary
        } else {
            theme::active().text_secondary
        })
        .focus_visible(|s| s.border_color(theme::active().text_primary))
        .when(enabled, |b| {
            b.cursor_pointer()
                .hover(|s| s.bg(theme::active().surface_selected))
                .on_click(cx.listener(move |r, _, w, cx| action(r, w, cx)))
                .on_key_down(cx.listener(move |r, e: &KeyDownEvent, w, cx| {
                    if matches!(r.flow.hotkey_recording, RecordingState::Recording) {
                        r.on_key_down(e, w, cx);
                        cx.stop_propagation();
                        return;
                    }
                    if e.keystroke.modifiers == gpui::Modifiers::default()
                        && matches!(e.keystroke.key.as_str(), "enter" | "space")
                    {
                        keyboard(r, w, cx);
                        cx.stop_propagation();
                    }
                }))
        })
        .child(label.into())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn delayed_clipboard_ack_rejects_duplicate_activation() {
        let mut busy = false;
        assert!(!begin_clipboard_change(&mut busy, false));
        assert!(!busy);
        assert!(begin_clipboard_change(&mut busy, true));
        assert!(!begin_clipboard_change(&mut busy, true));
        assert!(busy);
    }
    #[test]
    fn leaving_mac_cancels_recording() {
        assert_eq!(
            recording_after_navigation(SetupStep::Welcome, RecordingState::Recording),
            RecordingState::Idle
        );
        assert_eq!(
            recording_after_navigation(SetupStep::Mac, RecordingState::Recording),
            RecordingState::Recording
        );
    }
    #[test]
    fn evidence_steps_are_one_based_and_invalid_values_start_at_welcome() {
        assert_eq!(evidence_step(Some("1")), SetupStep::Welcome);
        assert_eq!(evidence_step(Some("3")), SetupStep::Workspace);
        for value in [None, Some("0"), Some("4"), Some("6"), Some("oops")] {
            assert_eq!(evidence_step(value), SetupStep::Welcome);
        }
    }
    #[test]
    fn first_run_has_only_three_visible_steps() {
        assert_eq!(SetupStep::ALL.len(), 3);
        assert_eq!(SetupStep::Mac.next(), SetupStep::Workspace);
        assert_eq!(SetupStep::Workspace.next(), SetupStep::Workspace);
    }
    #[test]
    fn three_steps_have_bounded_back_and_next() {
        let mut step = SetupStep::Welcome;
        assert_eq!(step.back(), SetupStep::Welcome);
        for expected in [SetupStep::Mac, SetupStep::Workspace] {
            step = step.next();
            assert_eq!(step, expected);
        }
        assert_eq!(step.next(), SetupStep::Workspace);
        assert_eq!(step.back(), SetupStep::Mac);
    }
    #[test]
    fn choosing_an_existing_folder_reuses_its_workspace() {
        let folder = std::env::temp_dir().canonicalize().unwrap();
        let existing = Workspace {
            id: "existing".into(),
            name: "Existing".into(),
            repository: folder.to_string_lossy().into_owned(),
            instructions: String::new(),
            away_enabled: false,
        };
        assert!(matches!(
            folder_choice(&folder, &[existing]),
            Ok(FolderChoice::Existing(id)) if id == "existing"
        ));
    }
    #[test]
    fn choosing_a_new_folder_registers_only_a_workspace() {
        let folder = std::env::temp_dir().canonicalize().unwrap();
        let result = folder_choice(&folder, &[]).unwrap();
        let FolderChoice::New(workspace) = result else {
            panic!("expected a new workspace");
        };
        assert!(workspace.id.is_empty());
        assert_eq!(workspace.repository, folder.to_string_lossy());
        assert!(workspace.instructions.is_empty());
        assert!(!workspace.away_enabled);
    }
    #[test]
    fn choosing_a_file_does_not_register_a_workspace() {
        let file = std::env::current_exe().unwrap();
        assert!(folder_choice(&file, &[]).is_err());
    }
}

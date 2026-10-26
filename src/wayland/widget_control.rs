use super::*;

impl App {
    pub(super) fn panel_name(&self, panel: usize) -> &str {
        if panel < 3 {
            layout::NAMES[panel]
        } else {
            &self.config.plugins[panel - 3].id
        }
    }
    pub(super) fn is_hidden(&self, panel: usize) -> bool {
        self.overrides
            .0
            .get(self.panel_name(panel))
            .is_some_and(|o| o.hidden)
    }
    pub(super) fn is_locked(&self, panel: usize) -> bool {
        self.overrides
            .0
            .get(self.panel_name(panel))
            .and_then(|o| o.locked)
            .unwrap_or_else(|| panel >= 3 && self.config.plugins[panel - 3].locked)
    }
    pub(super) fn drag_height(&self, panel: usize) -> i32 {
        if self.is_locked(panel) {
            return 0;
        }
        if panel < 3 {
            return 42;
        }
        let p = &self.config.plugins[panel - 3];
        p.drag_height
            .unwrap_or(if p.decoration == config::Decoration::Card {
                42
            } else {
                0
            })
            .min(p.height)
    }
    pub(super) fn control_status(&self, panel: usize) -> serde_json::Value {
        let position = self.layout.position(panel);
        let external = panel >= 3
            && self.config.plugins[panel - 3].render_mode == config::RenderMode::External;
        let provider = self
            .plugins
            .iter()
            .find(|(i, _)| *i == panel)
            .map(|(_, r)| r.diagnostics());
        serde_json::json!({"id":self.panel_name(panel),"enabled":if panel<3 {self.config.panel_enabled(panel)}else{self.config.plugins[panel-3].enabled},
            "visible":if external {None}else{Some(self.config.panel_enabled(panel) && !self.is_hidden(panel))},"locked":self.is_locked(panel),
            "position":[position.x,position.y],"size":[self.config.panel_width(panel),self.config.panel_height(panel)],"provider":provider})
    }
    pub(super) fn list_status(&self, offset: usize, limit: usize) -> serde_json::Value {
        let total = self.config.panel_count();
        let start = offset.min(total);
        let end = start.saturating_add(limit.clamp(1, 32)).min(total);
        let widgets: Vec<_> = (start..end)
            .map(|panel| {
                let mut status = self.control_status(panel);
                if let Some(provider) = status.get_mut("provider").and_then(|v| v.as_object_mut()) {
                    provider.remove("stderr_tail");
                    if let Some(error) = provider.get("last_error").and_then(|v| v.as_str()) {
                        provider.insert(
                            "last_error".into(),
                            serde_json::Value::String(error.chars().take(128).collect()),
                        );
                    }
                }
                status
            })
            .collect();
        serde_json::json!({"widgets":widgets,"total":total,"next_offset":if end<total {Some(end)}else{None}})
    }

    pub(super) fn control(
        &mut self,
        request: rs_wwidget::ipc::Request,
        qh: &QueueHandle<Self>,
    ) -> Result<serde_json::Value> {
        use rs_wwidget::ipc::Request;
        if let Request::Status {
            id: None,
            offset,
            limit,
        } = &request
        {
            return Ok(self.list_status(offset.unwrap_or(0), limit.unwrap_or(32)));
        }
        let id = match &request {
            Request::Status { id: Some(id), .. }
            | Request::Reload { id }
            | Request::Configure { id, .. }
            | Request::Reset { id }
            | Request::Move { id, .. }
            | Request::Lock { id, .. }
            | Request::Visible { id, .. }
            | Request::Send { id, .. } => id,
            _ => unreachable!(),
        };
        let panel = (0..self.config.panel_count())
            .find(|i| self.panel_name(*i) == id)
            .ok_or("unknown widget ID")?;
        match request {
            Request::Status { .. } => {}
            Request::Reload { .. } => {
                let runtime = self
                    .plugins
                    .iter_mut()
                    .find(|(i, _)| *i == panel)
                    .ok_or("widget has no enabled provider")?;
                runtime.1.reload(Instant::now())?;
            }
            Request::Configure { options, .. } => {
                let runtime = self
                    .plugins
                    .iter_mut()
                    .find(|(i, _)| *i == panel)
                    .ok_or("widget has no enabled provider")?;
                runtime.1.configure(&options, Instant::now())?;
            }
            Request::Send { name, data, .. } => {
                if !rs_wwidget::registry::valid_id(&name) {
                    return Err("event name must be a valid 1..64 character ID".into());
                }
                let runtime = self
                    .plugins
                    .iter_mut()
                    .find(|(i, _)| *i == panel)
                    .ok_or("widget has no enabled provider")?;
                if runtime.1.config().input != config::Input::Interactive || !runtime.1.is_running()
                {
                    return Err("widget is not a running interactive provider".into());
                }
                self.input_event(panel, InputEvent::Custom { name, data });
                if !self
                    .plugins
                    .iter()
                    .find(|(i, _)| *i == panel)
                    .is_some_and(|(_, r)| r.is_running())
                {
                    return Err("event delivery failed; provider is restarting".into());
                }
            }
            command => {
                if panel >= 3
                    && self.config.plugins[panel - 3].render_mode == config::RenderMode::External
                {
                    return Err(
                        "external renderer owns its windows; use its own geometry/input controls"
                            .into(),
                    );
                }
                self.finish_drag(false);
                match command {
                    Request::Reset { .. } => {
                        let old = self.overrides.clone();
                        let name = self.panel_name(panel).to_string();
                        self.overrides.0.remove(&name);
                        if let Err(error) = self
                            .overrides
                            .save(&self.state_dir.join("widget-controls.json"))
                        {
                            self.overrides = old;
                            return Err(error);
                        }
                        self.release_panel_input(panel);
                        self.surfaces.retain(|s| s.panel != Some(panel));
                        if self.config.panel_enabled(panel) {
                            self.create_surface(Some(panel), qh);
                        }
                    }
                    Request::Move { x, y, .. } => {
                        let previous = self.layout.position(panel);
                        let next = layout::Position {
                            x: layout::snap(
                                x as f64,
                                self.geometry.0,
                                self.config.panel_width(panel),
                                0,
                            ),
                            y: layout::snap(
                                y as f64,
                                self.geometry.1,
                                self.config.panel_height(panel),
                                48,
                            ),
                        };
                        self.layout.set_position(panel, next);
                        if let Err(error) = self.layout.save(&self.state_dir.join("layout.ini")) {
                            self.layout.set_position(panel, previous);
                            return Err(error.into());
                        }
                        if let Some(surface) =
                            self.surfaces.iter_mut().find(|s| s.panel == Some(panel))
                        {
                            surface.layer.set_margin(next.y, 0, 0, next.x);
                            surface.wl.commit();
                        }
                    }
                    Request::Lock { locked, .. } => {
                        let old = self.overrides.clone();
                        self.overrides
                            .0
                            .entry(self.panel_name(panel).to_string())
                            .or_default()
                            .locked = Some(locked);
                        if let Err(error) = self
                            .overrides
                            .save(&self.state_dir.join("widget-controls.json"))
                        {
                            self.overrides = old;
                            return Err(error);
                        }
                        // Recreate just this surface to update its input region.
                        self.release_panel_input(panel);
                        self.surfaces.retain(|s| s.panel != Some(panel));
                        if self.config.panel_enabled(panel) {
                            self.create_surface(Some(panel), qh);
                        }
                    }
                    Request::Visible { visible, .. } => {
                        let old = self.overrides.clone();
                        self.overrides
                            .0
                            .entry(self.panel_name(panel).to_string())
                            .or_default()
                            .hidden = !visible;
                        if let Err(error) = self
                            .overrides
                            .save(&self.state_dir.join("widget-controls.json"))
                        {
                            self.overrides = old;
                            return Err(error);
                        }
                        self.release_panel_input(panel);
                        self.surfaces.retain(|s| s.panel != Some(panel));
                        if visible && self.config.panel_enabled(panel) {
                            self.create_surface(Some(panel), qh);
                        }
                    }
                    _ => unreachable!(),
                }
            }
        }
        self.status();
        Ok(self.control_status(panel))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn widget_lists_are_paged_and_oversized_offsets_are_safe() {
        let mut config = Config::default();
        for i in 0..80 {
            config.plugins.push(config::PluginConfig {
                id: format!("widget-{i}"),
                ..config::PluginConfig::default()
            });
        }
        let mut app = App::new(config);
        app.layout = Layout::defaults(1920, 1080).with_plugins(&app.config, 1920, 1080);
        let page = app.list_status(0, usize::MAX);
        assert_eq!(page["widgets"].as_array().unwrap().len(), 32);
        assert_eq!(page["next_offset"], 32);
        assert_eq!(page["total"], 83);
        assert!(serde_json::to_vec(&page).unwrap().len() < rs_wwidget::ipc::MAX_REPLY);
        let empty = app.list_status(usize::MAX, 32);
        assert!(empty["widgets"].as_array().unwrap().is_empty());
        assert!(empty["next_offset"].is_null());
    }
}

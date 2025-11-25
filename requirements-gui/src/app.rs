use eframe::egui;
use requirements_core::{
    Requirement, RequirementPriority, RequirementStatus, RequirementType,
    RequirementsStore, Storage, determine_requirements_path, Comment,
    HistoryEntry, FieldChange,
};
use std::collections::{HashSet, HashMap};
use chrono::Utc;
use uuid::Uuid;

#[derive(Default, PartialEq, Clone)]
enum DetailTab {
    #[default]
    Description,
    Comments,
    Links,
    History,
}

#[derive(Default, PartialEq, Clone)]
enum View {
    #[default]
    List,
    Detail,
    Add,
    Edit,
}

pub struct RequirementsApp {
    storage: Storage,
    store: RequirementsStore,
    current_view: View,
    selected_idx: Option<usize>,
    filter_text: String,
    active_tab: DetailTab,

    // Form state
    form_title: String,
    form_description: String,
    form_status: RequirementStatus,
    form_priority: RequirementPriority,
    form_type: RequirementType,
    form_owner: String,
    form_feature: String,
    form_tags: String,

    // Messages
    message: Option<(String, bool)>, // (message, is_error)

    // Comment state
    comment_author: String,
    comment_content: String,
    show_add_comment: bool,
    reply_to_comment: Option<Uuid>, // Parent comment ID for replies
    collapsed_comments: HashMap<Uuid, bool>, // Track which comments are collapsed
    edit_comment_id: Option<Uuid>,

    // Pending operations (to avoid borrow checker issues)
    pending_delete: Option<usize>,
    pending_view_change: Option<View>,
    pending_comment_add: Option<(String, String, Option<Uuid>)>, // (author, content, parent_id)
    pending_comment_delete: Option<Uuid>,
}

impl RequirementsApp {
    pub fn new(_cc: &eframe::CreationContext<'_>) -> Self {
        let requirements_path = determine_requirements_path(None)
            .unwrap_or_else(|_| std::path::PathBuf::from("requirements.yaml"));

        let storage = Storage::new(requirements_path);
        let store = storage.load().unwrap_or_else(|_| RequirementsStore::new());

        Self {
            storage,
            store,
            current_view: View::List,
            selected_idx: None,
            filter_text: String::new(),
            active_tab: DetailTab::Description,
            form_title: String::new(),
            form_description: String::new(),
            form_status: RequirementStatus::Draft,
            form_priority: RequirementPriority::Medium,
            form_type: RequirementType::Functional,
            form_owner: String::new(),
            form_feature: String::from("Uncategorized"),
            form_tags: String::new(),
            message: None,
            comment_author: String::new(),
            comment_content: String::new(),
            show_add_comment: false,
            reply_to_comment: None,
            collapsed_comments: HashMap::new(),
            edit_comment_id: None,
            pending_delete: None,
            pending_view_change: None,
            pending_comment_add: None,
            pending_comment_delete: None,
        }
    }

    fn reload(&mut self) {
        if let Ok(store) = self.storage.load() {
            self.store = store;
            self.message = Some(("Reloaded successfully".to_string(), false));
        } else {
            self.message = Some(("Failed to reload".to_string(), true));
        }
    }

    fn save(&mut self) {
        if let Err(e) = self.storage.save(&self.store) {
            self.message = Some((format!("Error saving: {}", e), true));
        } else {
            self.message = Some(("Saved successfully".to_string(), false));
        }
    }

    fn clear_form(&mut self) {
        self.form_title.clear();
        self.form_description.clear();
        self.form_status = RequirementStatus::Draft;
        self.form_priority = RequirementPriority::Medium;
        self.form_type = RequirementType::Functional;
        self.form_owner.clear();
        self.form_feature = String::from("Uncategorized");
        self.form_tags.clear();
    }

    fn load_form_from_requirement(&mut self, idx: usize) {
        if let Some(req) = self.store.requirements.get(idx) {
            self.form_title = req.title.clone();
            self.form_description = req.description.clone();
            self.form_status = req.status.clone();
            self.form_priority = req.priority.clone();
            self.form_type = req.req_type.clone();
            self.form_owner = req.owner.clone();
            self.form_feature = req.feature.clone();
            let tags_vec: Vec<String> = req.tags.iter().cloned().collect();
            self.form_tags = tags_vec.join(", ");
        }
    }

    fn add_requirement(&mut self) {
        let tags: HashSet<String> = self.form_tags
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();

        let mut req = Requirement::new(
            self.form_title.clone(),
            self.form_description.clone(),
        );
        req.status = self.form_status.clone();
        req.priority = self.form_priority.clone();
        req.req_type = self.form_type.clone();
        req.owner = self.form_owner.clone();
        req.feature = self.form_feature.clone();
        req.tags = tags;

        self.store.add_requirement(req);
        self.save();
        self.clear_form();
        self.current_view = View::List;
        self.message = Some(("Requirement added successfully".to_string(), false));
    }

    fn update_requirement(&mut self, idx: usize) {
        if let Some(req) = self.store.requirements.get_mut(idx) {
            req.title = self.form_title.clone();
            req.description = self.form_description.clone();
            req.status = self.form_status.clone();
            req.priority = self.form_priority.clone();
            req.req_type = self.form_type.clone();
            req.owner = self.form_owner.clone();
            req.feature = self.form_feature.clone();
            req.tags = self.form_tags
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect();
            req.modified_at = Utc::now();

            self.save();
            self.clear_form();
            self.current_view = View::Detail;
            self.message = Some(("Requirement updated successfully".to_string(), false));
        }
    }

    fn delete_requirement(&mut self, idx: usize) {
        if idx < self.store.requirements.len() {
            self.store.requirements.remove(idx);
            self.save();
            self.selected_idx = None;
            self.current_view = View::List;
            self.message = Some(("Requirement deleted successfully".to_string(), false));
        }
    }

    fn show_top_panel(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::top("top_panel").show(ctx, |ui| {
            egui::menu::bar(ui, |ui| {
                if ui.button("➕ Add").clicked() {
                    self.clear_form();
                    self.pending_view_change = Some(View::Add);
                }

                if ui.button("🔄 Reload").clicked() {
                    self.reload();
                }

                ui.separator();
                ui.label(format!("Requirements: {}", self.store.requirements.len()));

                // Show message
                if let Some((msg, is_error)) = &self.message {
                    ui.separator();
                    let color = if *is_error { egui::Color32::RED } else { egui::Color32::GREEN };
                    ui.colored_label(color, msg);
                }
            });
        });
    }

    fn show_list_panel(&mut self, ctx: &egui::Context) {
        egui::SidePanel::left("list_panel").min_width(400.0).show(ctx, |ui| {
            ui.heading("Requirements");
            ui.separator();

            ui.horizontal(|ui| {
                ui.label("Search:");
                ui.text_edit_singleline(&mut self.filter_text);
            });

            ui.separator();

            egui::ScrollArea::vertical().show(ui, |ui| {
                for (idx, req) in self.store.requirements.iter().enumerate() {
                    if !self.filter_text.is_empty() {
                        let search = self.filter_text.to_lowercase();
                        if !req.title.to_lowercase().contains(&search)
                            && !req.description.to_lowercase().contains(&search) {
                            continue;
                        }
                    }

                    let selected = self.selected_idx == Some(idx);
                    if ui.selectable_label(selected, format!("{} - {}",
                        req.spec_id.as_deref().unwrap_or("N/A"),
                        &req.title
                    )).clicked() {
                        self.selected_idx = Some(idx);
                        self.pending_view_change = Some(View::Detail);
                    }
                }
            });
        });
    }

    fn show_detail_view(&mut self, ui: &mut egui::Ui) {
        if let Some(idx) = self.selected_idx {
            if let Some(req) = self.store.requirements.get(idx).cloned() {
                // Buttons need mutable access, so handle them separately
                let mut load_edit = false;
                let mut delete_req = false;

                ui.horizontal(|ui| {
                    ui.heading(&req.title);
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button("✏ Edit").clicked() {
                            load_edit = true;
                        }
                        if ui.button("🗑 Delete").clicked() {
                            delete_req = true;
                        }
                    });
                });

                if load_edit {
                    self.load_form_from_requirement(idx);
                    self.pending_view_change = Some(View::Edit);
                }
                if delete_req {
                    self.pending_delete = Some(idx);
                }

                ui.separator();

                egui::Grid::new("detail_grid")
                    .num_columns(2)
                    .spacing([40.0, 8.0])
                    .striped(true)
                    .show(ui, |ui| {
                        ui.label("SPEC-ID:");
                        ui.label(req.spec_id.as_deref().unwrap_or("N/A"));
                        ui.end_row();

                        ui.label("Status:");
                        ui.label(format!("{:?}", req.status));
                        ui.end_row();

                        ui.label("Priority:");
                        ui.label(format!("{:?}", req.priority));
                        ui.end_row();

                        ui.label("Type:");
                        ui.label(format!("{:?}", req.req_type));
                        ui.end_row();

                        ui.label("Feature:");
                        ui.label(&req.feature);
                        ui.end_row();

                        ui.label("Owner:");
                        ui.label(&req.owner);
                        ui.end_row();

                        if !req.tags.is_empty() {
                            ui.label("Tags:");
                            let tags_vec: Vec<String> = req.tags.iter().cloned().collect();
                            ui.label(tags_vec.join(", "));
                            ui.end_row();
                        }
                    });

                ui.separator();
                ui.heading("Description");
                ui.label(&req.description);

                if !req.relationships.is_empty() {
                    ui.separator();
                    ui.heading(format!("Relationships ({})", req.relationships.len()));
                    for rel in &req.relationships {
                        // Try to find the target requirement to show its spec_id
                        let target_label = self.store.requirements.iter()
                            .find(|r| r.id == rel.target_id)
                            .and_then(|r| r.spec_id.as_ref())
                            .map(|s| s.as_str())
                            .unwrap_or("Unknown");
                        ui.label(format!("• {:?} {}", rel.rel_type, target_label));
                    }
                }

                // Comments section
                ui.separator();
                ui.horizontal(|ui| {
                    ui.heading(format!("Comments ({})", req.comments.len()));
                    if ui.button("➕ Add Comment").clicked() {
                        self.show_add_comment = true;
                        self.reply_to_comment = None;
                        self.comment_author.clear();
                        self.comment_content.clear();
                    }
                });

                if self.show_add_comment {
                    self.show_comment_form(ui, idx);
                }

                // Display comments in a scrollable area
                egui::ScrollArea::vertical().max_height(400.0).show(ui, |ui| {
                    for comment in &req.comments {
                        self.show_comment_tree(ui, comment, idx, 0);
                    }
                });
            }
        } else {
            ui.vertical_centered(|ui| {
                ui.add_space(100.0);
                ui.heading("Select a requirement from the list");
            });
        }
    }

    fn show_form(&mut self, ui: &mut egui::Ui, is_edit: bool) {
        let title = if is_edit { "Edit Requirement" } else { "Add Requirement" };
        ui.heading(title);
        ui.separator();

        egui::ScrollArea::vertical().show(ui, |ui| {
            egui::Grid::new("form_grid")
                .num_columns(2)
                .spacing([40.0, 8.0])
                .show(ui, |ui| {
                    ui.label("Title:");
                    ui.text_edit_singleline(&mut self.form_title);
                    ui.end_row();

                    ui.label("Description:");
                    ui.text_edit_multiline(&mut self.form_description);
                    ui.end_row();

                    ui.label("Status:");
                    egui::ComboBox::new("status_combo", "")
                        .selected_text(format!("{:?}", self.form_status))
                        .show_ui(ui, |ui| {
                            ui.selectable_value(&mut self.form_status, RequirementStatus::Draft, "Draft");
                            ui.selectable_value(&mut self.form_status, RequirementStatus::Approved, "Approved");
                            ui.selectable_value(&mut self.form_status, RequirementStatus::Completed, "Completed");
                            ui.selectable_value(&mut self.form_status, RequirementStatus::Rejected, "Rejected");
                        });
                    ui.end_row();

                    ui.label("Priority:");
                    egui::ComboBox::new("priority_combo", "")
                        .selected_text(format!("{:?}", self.form_priority))
                        .show_ui(ui, |ui| {
                            ui.selectable_value(&mut self.form_priority, RequirementPriority::High, "High");
                            ui.selectable_value(&mut self.form_priority, RequirementPriority::Medium, "Medium");
                            ui.selectable_value(&mut self.form_priority, RequirementPriority::Low, "Low");
                        });
                    ui.end_row();

                    ui.label("Type:");
                    egui::ComboBox::new("type_combo", "")
                        .selected_text(format!("{:?}", self.form_type))
                        .show_ui(ui, |ui| {
                            ui.selectable_value(&mut self.form_type, RequirementType::Functional, "Functional");
                            ui.selectable_value(&mut self.form_type, RequirementType::NonFunctional, "NonFunctional");
                            ui.selectable_value(&mut self.form_type, RequirementType::System, "System");
                            ui.selectable_value(&mut self.form_type, RequirementType::User, "User");
                        });
                    ui.end_row();

                    ui.label("Owner:");
                    ui.text_edit_singleline(&mut self.form_owner);
                    ui.end_row();

                    ui.label("Feature:");
                    ui.text_edit_singleline(&mut self.form_feature);
                    ui.end_row();

                    ui.label("Tags (comma-separated):");
                    ui.text_edit_singleline(&mut self.form_tags);
                    ui.end_row();
                });

            ui.separator();
            ui.horizontal(|ui| {
                if ui.button("💾 Save").clicked() {
                    if is_edit {
                        if let Some(idx) = self.selected_idx {
                            self.update_requirement(idx);
                        }
                    } else {
                        self.add_requirement();
                    }
                }
                if ui.button("❌ Cancel").clicked() {
                    self.clear_form();
                    self.pending_view_change = Some(if is_edit { View::Detail } else { View::List });
                }
            });
        });
    }

    fn show_comment_form(&mut self, ui: &mut egui::Ui, req_idx: usize) {
        ui.group(|ui| {
            ui.label(if self.reply_to_comment.is_some() { "Add Reply" } else { "Add Comment" });

            ui.horizontal(|ui| {
                ui.label("Author:");
                ui.text_edit_singleline(&mut self.comment_author);
            });

            ui.label("Content:");
            ui.text_edit_multiline(&mut self.comment_content);

            ui.horizontal(|ui| {
                if ui.button("💾 Save").clicked() {
                    if !self.comment_author.is_empty() && !self.comment_content.is_empty() {
                        self.pending_comment_add = Some((
                            self.comment_author.clone(),
                            self.comment_content.clone(),
                            self.reply_to_comment,
                        ));
                        self.show_add_comment = false;
                    }
                }
                if ui.button("❌ Cancel").clicked() {
                    self.show_add_comment = false;
                    self.reply_to_comment = None;
                    self.comment_author.clear();
                    self.comment_content.clear();
                }
            });
        });
    }

    fn show_comment_tree(&mut self, ui: &mut egui::Ui, comment: &Comment, req_idx: usize, depth: usize) {
        let indent = depth as f32 * 20.0;
        ui.add_space(indent);

        let is_collapsed = self.collapsed_comments.get(&comment.id).copied().unwrap_or(false);

        ui.group(|ui| {
            ui.horizontal(|ui| {
                // Collapse/expand button if there are replies
                if !comment.replies.is_empty() {
                    let button_text = if is_collapsed { "▶" } else { "▼" };
                    if ui.small_button(button_text).clicked() {
                        self.collapsed_comments.insert(comment.id, !is_collapsed);
                    }
                } else {
                    ui.label("  "); // Spacing when no collapse button
                }

                ui.label(format!("👤 {}", comment.author));
                ui.label(format!("🕒 {}", comment.created_at.format("%Y-%m-%d %H:%M")));
            });

            ui.label(&comment.content);

            ui.horizontal(|ui| {
                if ui.small_button("💬 Reply").clicked() {
                    self.show_add_comment = true;
                    self.reply_to_comment = Some(comment.id);
                    self.comment_author.clear();
                    self.comment_content.clear();
                }
                if ui.small_button("🗑 Delete").clicked() {
                    self.pending_comment_delete = Some(comment.id);
                }
            });
        });

        // Show replies if not collapsed
        if !is_collapsed {
            for reply in &comment.replies {
                self.show_comment_tree(ui, reply, req_idx, depth + 1);
            }
        }
    }

    fn add_comment_to_requirement(&mut self, idx: usize, author: String, content: String, parent_id: Option<Uuid>) {
        if let Some(req) = self.store.requirements.get_mut(idx) {
            if let Some(parent) = parent_id {
                // This is a reply
                let reply = Comment::new_reply(author, content, parent);
                if let Err(e) = req.add_reply(parent, reply) {
                    self.message = Some((format!("Error adding reply: {}", e), true));
                    return;
                }
            } else {
                // This is a top-level comment
                let comment = Comment::new(author, content);
                req.add_comment(comment);
            }

            self.save();
            self.comment_author.clear();
            self.comment_content.clear();
            self.reply_to_comment = None;
            self.message = Some(("Comment added successfully".to_string(), false));
        }
    }

    fn delete_comment_from_requirement(&mut self, idx: usize, comment_id: Uuid) {
        if let Some(req) = self.store.requirements.get_mut(idx) {
            if let Err(e) = req.delete_comment(&comment_id) {
                self.message = Some((format!("Error deleting comment: {}", e), true));
                return;
            }
            self.save();
            self.message = Some(("Comment deleted successfully".to_string(), false));
        }
    }
}

impl eframe::App for RequirementsApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Handle pending operations (to avoid borrow checker issues)
        if let Some(idx) = self.pending_delete.take() {
            self.delete_requirement(idx);
        }
        if let Some(view) = self.pending_view_change.take() {
            self.current_view = view;
        }
        if let Some((author, content, parent_id)) = self.pending_comment_add.take() {
            if let Some(idx) = self.selected_idx {
                self.add_comment_to_requirement(idx, author, content, parent_id);
            }
        }
        if let Some(comment_id) = self.pending_comment_delete.take() {
            if let Some(idx) = self.selected_idx {
                self.delete_comment_from_requirement(idx, comment_id);
            }
        }

        self.show_top_panel(ctx);

        // Only show list panel when not in form view
        if self.current_view == View::List || self.current_view == View::Detail {
            self.show_list_panel(ctx);
        }

        egui::CentralPanel::default().show(ctx, |ui| {
            match &self.current_view {
                View::List | View::Detail => {
                    self.show_detail_view(ui);
                }
                View::Add => {
                    self.show_form(ui, false);
                }
                View::Edit => {
                    self.show_form(ui, true);
                }
            }
        });
    }
}

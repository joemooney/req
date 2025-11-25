// Simplified GUI to get working - will enhance later
use eframe::egui;
use requirements_core::{
    Requirement, RequirementPriority, RequirementStatus, RequirementType,
    RequirementsStore, Storage, determine_requirements_path,
};
use uuid::Uuid;

pub struct RequirementsApp {
    storage: Storage,
    store: RequirementsStore,
    selected_idx: Option<usize>,
    filter_text: String,
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
            selected_idx: None,
            filter_text: String::new(),
        }
    }

    fn reload(&mut self) {
        if let Ok(store) = self.storage.load() {
            self.store = store;
        }
    }
}

impl eframe::App for RequirementsApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::TopBottomPanel::top("top_panel").show(ctx, |ui| {
            egui::menu::bar(ui, |ui| {
                if ui.button("Reload").clicked() {
                    self.reload();
                }
                ui.label(format!("Requirements: {}", self.store.requirements.len()));
            });
        });

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
                    }
                }
            });
        });

        egui::CentralPanel::default().show(ctx, |ui| {
            if let Some(idx) = self.selected_idx {
                if let Some(req) = self.store.requirements.get(idx) {
                    ui.heading(&req.title);
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
                        });

                    ui.separator();
                    ui.heading("Description");
                    ui.label(&req.description);

                    if !req.relationships.is_empty() {
                        ui.separator();
                        ui.heading(format!("Relationships ({})", req.relationships.len()));
                        for rel in &req.relationships {
                            ui.label(format!("• {} {}", rel.rel_type, rel.target_id));
                        }
                    }
                } else {
                    ui.label("Requirement not found");
                }
            } else {
                ui.vertical_centered(|ui| {
                    ui.add_space(100.0);
                    ui.heading("Select a requirement from the list");
                });
            }
        });
    }
}

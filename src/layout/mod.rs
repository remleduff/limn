use cassowary::Variable;
use multi_mut::HashMapMultiMut;

use limn_layout::linear_layout::{LinearLayoutHandler, Orientation};

use resources::WidgetId;

use ui::Ui;
use event::Target;

use widget::{Widget, WidgetBuilder, WidgetBuilderCore};

use self::container::LayoutContainer;

pub mod container;

pub use self::solver::LimnSolver;
pub use limn_layout::*;

impl LayoutContainer for LinearLayoutHandler {
    fn set_padding(&mut self, padding: f64) {
        self.padding = padding;
    }
    fn add_child(&mut self, parent: &mut Widget, child: &mut WidgetBuilder, solver: &mut LayoutManager) {
        let child_id = child.id();
        solver.update_layouts(child.id(), parent.id, |child, parent| {
            self.add_child_layout(&parent.vars, child, child_id.0);
        });
    }
    fn remove_child(&mut self, parent: &mut Widget, child_id: WidgetId, solver: &mut LayoutManager) {
        solver.update_layout(parent.id, |layout| {
            self.remove_child_layout(layout, child_id.0);
        });
    }
}

impl WidgetBuilder {
    pub fn vbox(&mut self) -> &mut Self {
        let handler = LinearLayoutHandler::new(Orientation::Vertical, &self.layout().vars);
        self.set_container(handler)
    }
    pub fn hbox(&mut self) -> &mut Self {
        let handler = LinearLayoutHandler::new(Orientation::Horizontal, &self.layout().vars);
        self.set_container(handler)
    }
}


/// wrapper around cassowary solver that keeps widgets positions in sync, sends events when layout changes happen
pub struct LayoutManager {
    pub solver: LimnSolver,
}

impl LayoutManager {
    pub fn new() -> Self {
        LayoutManager {
            solver: LimnSolver::new(),
        }
    }
    pub fn update_layout<F>(&mut self, id: WidgetId, f: F)
        where F: FnOnce(&mut Layout)
    {
        {
            let layout = self.solver.widget_vars.get_mut(&id.0).unwrap();
            f(layout);
        }
        self.solver.update_from_builder(id.0);
        self.check_changes();
    }
    pub fn update_layouts<F>(&mut self, id: WidgetId, id2: WidgetId, f: F)
        where F: FnOnce(&mut Layout, &mut Layout)
    {
        {
            let (layout, layout2) = self.solver.widget_vars.get_pair_mut(&id.0, &id2.0).unwrap();
            f(layout, layout2);
        }
        self.solver.update_from_builder(id.0);
        self.check_changes();
    }
    pub fn update_solver<F>(&mut self, f: F)
        where F: Fn(&mut LimnSolver)
    {
        f(&mut self.solver);
        self.check_changes();
    }

    pub fn check_changes(&mut self) {
        let changes = self.solver.fetch_changes();
        debug!("layout has {} changes", changes.len());
        if changes.len() > 0 {
            event!(Target::Ui, LayoutChanged(changes));
        }
    }
}

pub struct LayoutChanged(Vec<(usize, Variable, f64)>);
pub struct LayoutUpdated;

pub fn handle_layout_change(event: &LayoutChanged, ui: &mut Ui) {
    let ref changes = event.0;
    for &(widget_id, var, value) in changes {
        let widget_id = WidgetId(widget_id);
        if let Some(widget) = ui.graph.get_widget(widget_id) {
            let vars = &ui.layout.solver.widget_vars[&widget_id.0].vars;
            let var = vars.get_var(var).expect("Missing variable for widget");
            debug!("{:?}: {:?} = {}", widget.debug_name, var, value);
            match var {
                VarUpdate::Left => widget.bounds.origin.x = value,
                VarUpdate::Top => widget.bounds.origin.y = value,
                VarUpdate::Width => widget.bounds.size.width = value,
                VarUpdate::Height => widget.bounds.size.height = value,
            }
            event!(Target::Widget(widget_id), LayoutUpdated);
        }
    }
    // redraw everything when layout changes, for now
    ui.redraw();
}

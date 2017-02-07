use std::collections::{HashSet, HashMap};
use std::f64;
use std::any::Any;

use petgraph::stable_graph::StableGraph;
use petgraph::graph::NodeIndex;
use petgraph::visit::Dfs;
use petgraph::{Direction, Directed};
use petgraph::stable_graph::Neighbors;

use glutin;

use layout::LimnSolver;
use cassowary::Constraint;
use cassowary::strength::*;

use graphics::Context;

use backend::gfx::G2d;
use backend::glyph::GlyphCache;
use backend::window::Window;

use widget::Widget;
use widget::builder::WidgetBuilder;
use widget::layout::{self, LayoutBuilder, WidgetConstraint};
use event::{self, EventId, EventQueue, EventAddress, Hover};
use event::id::*;
use util::{self, Point, Rectangle, Dimensions};
use resources::WidgetId;
use color::*;

const DEBUG_BOUNDS: bool = false;

pub struct InputState {
    pub mouse: Point,
    pub last_over: HashSet<WidgetId>,
}
impl InputState {
    fn new() -> Self {
        InputState {
            mouse: Point { x: 0.0, y: 0.0 },
            last_over: HashSet::new(),
        }
    }
}

// special event handler for access to Ui
pub trait UiEventHandler {
    fn event_id(&self) -> EventId;
    fn handle_event(&mut self, args: UiEventArgs);
}
pub struct UiEventArgs<'a> {
    pub data: &'a (Any + 'static),
    pub ui: &'a mut Ui,
    pub event_queue: &'a mut EventQueue,
}

pub struct RedrawHandler {}
impl UiEventHandler for RedrawHandler {
    fn event_id(&self) -> EventId {
        REDRAW
    }
    fn handle_event(&mut self, args: UiEventArgs) {
        let ui = args.ui;
        ui.dirty_widgets.insert(ui.root_index.unwrap());
    }
}
pub struct LayoutHandler {}
impl UiEventHandler for LayoutHandler {
    fn event_id(&self) -> EventId {
        LAYOUT
    }
    fn handle_event(&mut self, args: UiEventArgs) {
        let ui = args.ui;
        let widget_id = args.data.downcast_ref::<WidgetId>().unwrap();
        let node_index = ui.find_widget(*widget_id).unwrap();
        let ref mut widget = ui.graph[node_index];
        widget.layout.update(&mut ui.solver);
    }
}

pub fn get_default_event_handlers() -> Vec<Box<UiEventHandler>> {
    vec!{Box::new(RedrawHandler{}), Box::new(LayoutHandler{})}
}

pub struct Ui {
    pub graph: StableGraph<Widget, ()>,
    pub root_index: Option<NodeIndex>,
    pub solver: LimnSolver,
    pub input_state: InputState,
    pub widget_map: HashMap<WidgetId, NodeIndex>,
    pub dirty_widgets: HashSet<NodeIndex>,
    pub glyph_cache: GlyphCache,
}
impl Ui {
    pub fn new(window: &mut Window, event_queue: &EventQueue) -> Self {
        Ui {
            graph: StableGraph::<Widget, ()>::new(),
            root_index: None,
            solver: LimnSolver::new(event_queue.clone()),
            input_state: InputState::new(),
            widget_map: HashMap::new(),
            dirty_widgets: HashSet::new(),
            glyph_cache: GlyphCache::new(&mut window.context.factory, 512, 512),
        }
    }
    pub fn resize_window_to_fit(&mut self, window: &Window) {
        let window_dims = self.get_root_dims();
        window.window.set_inner_size(window_dims.width as u32, window_dims.height as u32);
    }
    pub fn set_root(&mut self, mut root_widget: WidgetBuilder) {
        root_widget.layout.top_left(Point { x: 0.0, y: 0.0 }, None);
        self.root_index = Some(self.add_widget(root_widget, None));
        let ref mut root = &mut self.graph[self.root_index.unwrap()];
        self.solver.add_edit_variable(root.layout.right, STRONG).unwrap();
        self.solver.add_edit_variable(root.layout.bottom, STRONG).unwrap();
    }
    pub fn get_root(&mut self) -> &Widget {
        &self.graph[self.root_index.unwrap()]
    }
    pub fn get_root_dims(&mut self) -> Dimensions {
        let ref mut root = &mut self.graph[self.root_index.unwrap()];
        root.layout.update(&mut self.solver);
        root.layout.get_dims()
    }
    pub fn window_resized(&mut self, window_dims: Dimensions) {
        let ref mut root = self.graph[self.root_index.unwrap()];
        root.layout.update(&mut self.solver);
        self.solver.suggest_value(root.layout.right, window_dims.width).unwrap();
        self.solver.suggest_value(root.layout.bottom, window_dims.height).unwrap();
        self.dirty_widgets.insert(self.root_index.unwrap());
    }
    pub fn parents(&mut self, node_index: NodeIndex) -> Neighbors<()> {
        self.graph.neighbors_directed(node_index, Direction::Incoming)
    }
    pub fn children(&mut self, node_index: NodeIndex) -> Neighbors<()> {
        self.graph.neighbors_directed(node_index, Direction::Outgoing)
    }

    pub fn update_layout(&mut self) {
        let mut dfs = Dfs::new(&self.graph, self.root_index.unwrap());
        while let Some(node_index) = dfs.next(&self.graph) {
            let ref mut widget = self.graph[node_index];
            widget.layout.update(&mut self.solver);
        }
    }
    pub fn draw_node(&mut self,
                     context: Context,
                     graphics: &mut G2d,
                     node_index: NodeIndex,
                     crop_to: Rectangle) {

        let crop_to = {
            let ref mut widget = self.graph[node_index];
            widget.draw(crop_to,
                        &mut self.solver,
                        &mut self.glyph_cache,
                        context,
                        graphics);

            util::crop_rect(crop_to, widget.layout.bounds())
        };

        if !crop_to.no_area() {
            let children: Vec<NodeIndex> = self.children(node_index).collect();
            // need to iterate backwards to draw in correct order, because
            // petgraph neighbours iterate in reverse order of insertion, not sure why
            for child_index in children.iter().rev() {
                let child_index = child_index.clone();
                self.draw_node(context, graphics, child_index, crop_to);
            }
        }
    }
    pub fn draw(&mut self, context: Context, graphics: &mut G2d) {

        let index = self.root_index.unwrap().clone();
        let crop_to = Rectangle {
            top: 0.0,
            left: 0.0,
            width: f64::MAX,
            height: f64::MAX,
        };
        self.draw_node(context, graphics, index, crop_to);

        if DEBUG_BOUNDS {
            let mut dfs = Dfs::new(&self.graph, self.root_index.unwrap());
            while let Some(node_index) = dfs.next(&self.graph) {
                let ref widget = self.graph[node_index];
                let color = widget.debug_color.unwrap_or(GREEN);
                let bounds = widget.layout.bounds();
                util::draw_rect_outline(bounds, color, context, graphics);
            }
        }
    }
    pub fn add_widget(&mut self, mut widget: WidgetBuilder, parent_index: Option<NodeIndex>) -> NodeIndex {

        let (children, constraints, widget) = widget.build();
        self.solver.add_widget(&widget, constraints);

        let id = widget.id;
        let widget_index = self.graph.add_node(widget);
        if let Some(parent_index) = parent_index {
            self.graph.add_edge(parent_index, widget_index, ());
        }
        self.widget_map.insert(id, widget_index);
        self.dirty_widgets.insert(widget_index);
        for child in children {
            self.add_widget(*child, Some(widget_index));
        }
        widget_index
    }

    pub fn remove_widget(&mut self, widget_id: WidgetId) {
        if let Some(node_index) = self.find_widget(widget_id) {
            self.graph.remove_node(node_index);
            self.dirty_widgets.insert(self.root_index.unwrap());
            self.solver.remove_widget(&widget_id);
        }
    }
    pub fn get_widget(&self, widget_id: WidgetId) -> Option<&Widget> {
        self.widget_map.get(&widget_id).and_then(|node_index| {
            let ref widget = self.graph[NodeIndex::new(node_index.index())];
            return Some(widget);
        });
        None
    }
    pub fn handle_event(&mut self, event: glutin::Event, event_queue: &mut EventQueue) {
        match event {
            glutin::Event::MouseMoved(x, y) => {
                let mouse = Point {
                    x: x as f64,
                    y: y as f64,
                };
                self.input_state.mouse = mouse;
                let last_over = self.input_state.last_over.clone();
                for last_over in last_over {
                    let last_over = last_over.clone();
                    if let Some(last_index) = self.find_widget(last_over) {
                        let ref mut widget = self.graph[last_index];
                        if !widget.is_mouse_over(&mut self.solver, self.input_state.mouse) {
                            event_queue.push(EventAddress::Widget(last_over),
                                             WIDGET_HOVER,
                                             Box::new(Hover::Out));
                            self.input_state.last_over.remove(&last_over);
                        }
                    }
                }
                event_queue.push(EventAddress::UnderMouse,
                                 WIDGET_HOVER,
                                 Box::new(Hover::Over));
            }
            _ => (),
        }
        let ref root_widget = self.graph[self.root_index.unwrap()];
        let all_widgets = EventAddress::SubTree(root_widget.id);
        match event {
            glutin::Event::MouseWheel(..) => {
                event_queue.push(EventAddress::UnderMouse, WIDGET_MOUSE_WHEEL, Box::new(event.clone()));
                event_queue.push(all_widgets, MOUSE_WHEEL, Box::new(event));
            },
            glutin::Event::MouseInput(..) => {
                event_queue.push(EventAddress::UnderMouse, WIDGET_MOUSE_BUTTON, Box::new(event.clone()));
                event_queue.push(all_widgets, MOUSE_BUTTON, Box::new(event));
            },
            glutin::Event::MouseMoved(..) => {
                event_queue.push(all_widgets, MOUSE_MOVED, Box::new(event));
            }, _ => (),
        }
    }

    pub fn check_layout(&mut self, event_queue: &mut EventQueue) {
        // if layout has changed, send new mouse event, in case widget under mouse has shifted
        let has_changes = self.solver.fetch_changes().len() > 0;
        if has_changes {
            //self.solver.debug_constraints();
            let mouse = self.input_state.mouse;
            let event = glutin::Event::MouseMoved(mouse.x as i32, mouse.y as i32);
            self.handle_event(event, event_queue);
        }
    }
    pub fn is_mouse_over(&mut self, node_index: NodeIndex) -> bool {
        let ref mut widget = self.graph[node_index];
        widget.is_mouse_over(&mut self.solver, self.input_state.mouse)
    }
    pub fn find_widget(&mut self, widget_id: WidgetId) -> Option<NodeIndex> {
        self.widget_map.get(&widget_id).map(|index| *index)
    }

    pub fn trigger_widget_event(&mut self,
                            node_index: NodeIndex,
                            event_id: EventId,
                            data: &(Any + 'static),
                            event_queue: &mut EventQueue) -> bool {
        let ref mut widget = self.graph[node_index];
        let handled = widget.trigger_event(event_id, data, event_queue, &mut self.solver, &self.input_state);
        if let Some(ref mut drawable) = widget.drawable {
            if drawable.has_updated {
                self.dirty_widgets.insert(node_index);
                drawable.has_updated = false;
            }
        }
        handled
    }
}

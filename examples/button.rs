extern crate limn;
#[macro_use]
extern crate limn_layout;

mod util;

use limn::widgets::button::ToggleButtonBuilder;

fn main() {
    let app = util::init_default("Limn button demo");
    let mut root = app.ui.root.clone();

    let mut button = ToggleButtonBuilder::new();
    button.set_text("ON", "OFF");
    button.set_debug_name("button");
    layout!(button:
        center(&root),
        bound_by(&root).padding(50.0));
    root.add_child(button);

    app.main_loop();
}

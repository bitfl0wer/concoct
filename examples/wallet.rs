use accesskit::{Node, NodeId, Role};
use concoct::{composer::Composer, semantics::LayoutNode, Semantics, Widget};
use concoct::{container, render::run, Modifier};
use skia_safe::RGB;
use skia_safe::{Color4f, ColorSpace, Font, FontStyle, Paint, TextBlob, Typeface};
use std::sync::atomic::{AtomicU32, Ordering};
use std::{any, panic::Location, sync::Arc};
use taffy::{
    prelude::{AvailableSpace, Size},
    style::Style,
};

fn app() {
    container(Modifier::default(), || {
        flex_text("Hello");
    })
}

fn main() {
    run(app)
}

#[track_caller]
pub fn flex_text(string: impl Into<String>) {
    let location = Location::caller();
    Composer::with(|composer| {
        let mut cx = composer.borrow_mut();
        let id = cx.id(location);

        let typeface = Typeface::new("serif", FontStyle::bold()).unwrap();

        if let Some(widget) = cx.get_mut::<TextWidget>(&id) {
            widget.text = string.into();
            cx.children.push(id);
        } else {
            let widget = TextWidget {
                text: string.into(),
                node_id: None,
                layout_id: None,
                typeface,
                font_size: Arc::new(AtomicU32::new(400)),
            };
            cx.insert(id, widget, None);
        }
    })
}

pub struct TextWidget {
    text: String,
    node_id: Option<NodeId>,
    layout_id: Option<LayoutNode>,
    typeface: Typeface,
    font_size: Arc<AtomicU32>,
}

impl Widget for TextWidget {
    fn semantics(&mut self, semantics: &mut Semantics) {
        let node = Node {
            role: Role::StaticText,
            value: Some(self.text.clone().into_boxed_str()),
            ..Node::default()
        };

        if let Some(node_id) = self.node_id {
            semantics.update(node_id, node);
        } else {
            let id = semantics.insert(node);
            self.node_id = Some(id);
        }

        if let Some(_layout_id) = self.layout_id {
        } else {
            let font_size = self.font_size.clone();
            let typeface = self.typeface.clone();
            let text = self.text.clone();
            let layout_id = semantics.insert_layout_with_measure(
                Style::default(),
                move |_known_dimensions, available_space| {
                    let max_width = match available_space.width {
                        AvailableSpace::Definite(px) => px,
                        AvailableSpace::MaxContent => f32::MAX,
                        AvailableSpace::MinContent => f32::MIN,
                    };
                    let max_height = match available_space.height {
                        AvailableSpace::Definite(px) => px,
                        AvailableSpace::MaxContent => f32::MAX,
                        AvailableSpace::MinContent => f32::MIN,
                    };

                    let mut font_size_value = font_size.load(Ordering::SeqCst);
                    loop {
                        let font = Font::new(&typeface, font_size_value as f32);
                        let (_, bounds) = font.measure_str(&text, None);

                        if bounds.width() <= max_width && bounds.height() <= max_height {
                            break;
                        }

                        font_size_value -= 10;
                    }

                    font_size.store(font_size_value, Ordering::SeqCst);

                    Size {
                        width: max_width,
                        height: max_height,
                    }
                },
            );
            self.layout_id = Some(layout_id);
        }
    }

    fn paint(&mut self, semantics: &Semantics, canvas: &mut skia_safe::Canvas) {
        let paint = Paint::new(Color4f::from(RGB::from((0, 0, 0))), &ColorSpace::new_srgb());

        let font = Font::new(&self.typeface, self.font_size.load(Ordering::SeqCst) as f32);
        let text_blob = TextBlob::new(&self.text, &font).unwrap();

        let layout = semantics.taffy.layout(self.layout_id.unwrap()).unwrap();

        canvas.draw_text_blob(
            &text_blob,
            (
                layout.location.x,
                layout.location.y + text_blob.bounds().height(),
            ),
            &paint,
        );
    }

    fn remove(&mut self, semantics: &mut Semantics) {
        if let Some(node_id) = self.node_id {
            semantics.remove(node_id);
        }
    }

    fn any(&self) -> &dyn any::Any {
        self
    }

    fn any_mut(&mut self) -> &mut dyn any::Any {
        self
    }
}

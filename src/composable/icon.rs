use super::Container;
use crate::{modify::ModifyExt, Modifier, View};
use skia_safe::{Matrix, Paint, Path};
use taffy::prelude::Size;

pub struct Icon {
    path: Path,
    paint: Paint,
}

impl Icon {
    pub fn build(svg: &str, paint: Paint) -> Self {
        let path = Path::from_svg(svg).unwrap();
        Self { path, paint }
    }
}

impl View for Icon {
    #[track_caller]
    fn view(self) {
        Container::build_row(|| {})
            .size(Size::from_points(50., 50.))
            .modifier(Modifier.draw(move |layout, canvas| {
                let translation = Matrix::translate((layout.location.x, layout.location.y));
                canvas.draw_path(&self.path.with_transform(&translation), &self.paint);
            }))
            .view();
    }
}

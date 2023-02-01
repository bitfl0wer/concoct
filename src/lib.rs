use std::any::Any;

pub mod composer;
pub use composer::Composer;

mod container;
pub use container::container;

pub mod modify;
pub use modify::{Modifier, Modify};

pub mod render;

mod semantics;
pub use semantics::Semantics;

pub mod state;

mod tester;
pub use tester::Tester;

mod text;
pub use text::text;

pub trait Widget: Any {
    fn semantics(&mut self, semantics: &mut Semantics);

    fn remove(&mut self, semantics: &mut Semantics);

    fn any(&self) -> &dyn Any;

    fn any_mut(&mut self) -> &mut dyn Any;
}

use std::{any::Any, marker::PhantomData};

pub mod composer;
pub use composer::Composer;

mod container;
pub use container::container;

mod semantics;
pub use semantics::Semantics;

mod text;
pub use text::text;

pub trait Widget: Any {
    fn semantics(&mut self, semantics: &mut Semantics);

    fn any_mut(&mut self) -> &mut dyn Any;
}

pub struct Modifier<T, M> {
    modify: M,
    _marker: PhantomData<T>,
}

impl<T> Default for Modifier<T, ()> {
    fn default() -> Self {
        Self::new(())
    }
}

impl<T, M> Modifier<T, M> {
    pub fn new(modify: M) -> Self {
        Self {
            modify,
            _marker: PhantomData,
        }
    }

    pub fn chain<B>(self, modify: B) -> Modifier<T, Chain<M, B>> {
        Modifier::new(Chain {
            a: self.modify,
            b: modify,
        })
    }

    pub fn merge_descendants(self) -> Modifier<T, Chain<M, MergeDescendants>> {
        self.chain(MergeDescendants)
    }
}

pub trait Modify<T> {
    fn modify(&mut self, value: &mut T);
}

impl<T> Modify<T> for () {
    fn modify(&mut self, _value: &mut T) {}
}

pub struct Chain<A, B> {
    a: A,
    b: B,
}

impl<T, A: Modify<T>, B: Modify<T>> Modify<T> for Chain<A, B> {
    fn modify(&mut self, value: &mut T) {
        self.a.modify(value);
        self.b.modify(value);
    }
}

pub struct ContainerModifier {
    merge_descendants: bool,
}

pub struct MergeDescendants;

impl Modify<ContainerModifier> for MergeDescendants {
    fn modify(&mut self, value: &mut ContainerModifier) {
        value.merge_descendants = true;
    }
}

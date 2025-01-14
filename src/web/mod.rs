use crate::view::View;
use crate::Platform;
use std::{cell::RefCell, rc::Rc};
pub use web_sys::Element;
use web_sys::{Document, Node};

mod event_ext;
pub use event_ext::EventExt;

pub mod html;
pub use self::html::{Html, ClassList};

mod on;
pub use self::on::{on, On};

mod value;
pub use self::value::{value, Value};

mod attr;
pub use attr::{attr, class};

pub struct Web<E> {
    pub document: Document,
    stack: Vec<(web_sys::Element, usize)>,
    pub update: Rc<RefCell<Option<Box<dyn FnMut(E)>>>>,
}

impl<E> Platform for Web<E> {
    type Event = E;

    fn advance(&mut self) {
        let (_, idx) = self.stack.last_mut().unwrap();
        *idx += 1;
    }
}

impl<E> Web<E> {
    pub fn new() -> Self {
        let window = web_sys::window().expect("no global `window` exists");
        let document = window.document().expect("should have a document on window");
        let body = document.body().expect("HTML document missing body");

        Self {
            document,
            stack: vec![(body.into(), 0)],
            update: Rc::new(RefCell::new(None)),
        }
    }

    pub fn insert(&mut self, node: &Node) {
        let (parent, idx) = self.stack.last_mut().unwrap();
        parent
            .insert_before(node, parent.children().get_with_index(*idx as _).as_deref())
            .unwrap();

        *idx += 1;
    }

    pub fn with_nested<R>(
        &mut self,
        elem: Element,
        f: impl FnOnce(&mut Self) -> R,
    ) -> (Element, usize, R) {
        self.stack.push((elem, 0));
        let output = f(self);
        let (elem, count) = self.stack.pop().unwrap();
        (elem, count, output)
    }
}

pub fn run<T, E, V>(state: T, update: impl Fn(&mut T, E) + 'static, f: impl Fn(&T) -> V + 'static)
where
    T: 'static,
    E: 'static,
    V: View<Web<E>>,
    V::State: 'static,
{
    let f = Rc::new(f);

    let state = Rc::new(RefCell::new(state));
    let view_state: Rc<RefCell<Option<V::State>>> = Rc::new(RefCell::new(None));

    let cx_state = state.clone();
    let cx_f = f.clone();
    let cx_view_state = view_state.clone();

    let cx = Rc::new(RefCell::new(Web::new()));
    let update_cx = cx.clone();
    *cx.borrow_mut().update.borrow_mut() = Some(Box::new(move |msg| {
        update(&mut cx_state.borrow_mut(), msg);

        let view = cx_f(&cx_state.borrow());
        let update_cx = &mut update_cx.borrow_mut();
        update_cx.stack.last_mut().unwrap().1 = 0;
        view.rebuild(update_cx, cx_view_state.borrow_mut().as_mut().unwrap());
    }));

    let view = f(&state.borrow());
    *view_state.borrow_mut() = Some(view.build(&mut cx.borrow_mut()));
}

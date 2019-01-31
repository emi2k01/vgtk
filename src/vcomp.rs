use gtk::Container;

use std::any::{Any, TypeId};
use std::cell::RefCell;
use std::marker::PhantomData;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::callback::Callback;
use crate::component::{Component, View};
use crate::scope::Scope;
use crate::vdom::ComponentState;

pub struct AnyProps {
    valid: AtomicBool,
    type_id: TypeId,
    data: *mut (),
}

impl AnyProps {
    pub fn new<Props: Any>(props: Props) -> Self {
        AnyProps {
            valid: AtomicBool::new(true),
            type_id: TypeId::of::<Props>(),
            data: Box::into_raw(Box::new(props)) as *mut (),
        }
    }

    pub fn unwrap<Props: Any>(&self) -> Props {
        if !self.valid.swap(false, Ordering::SeqCst) {
            panic!("tried to unwrap AnyProps of type {:?} twice", self.type_id)
        }
        if self.type_id != TypeId::of::<Props>() {
            panic!(
                "passed type {:?} to constructor expecting type {:?}",
                self.type_id,
                TypeId::of::<Props>()
            )
        }
        unsafe { *Box::from_raw(self.data as *mut Props) }
    }
}

type Constructor<Model> = Fn(&AnyProps, Option<&Container>, &Scope<Model>) -> ComponentState<Model>;
type LazyActivator<Model> = Rc<RefCell<Option<Scope<Model>>>>;

pub struct VComponent<Model: Component> {
    parent: PhantomData<Model>,
    pub model_type: TypeId,
    pub props: AnyProps,
    pub constructor: Box<Constructor<Model>>,
    pub activators: Vec<LazyActivator<Model>>,
}

impl<Model: 'static + Component + View<Model>> VComponent<Model> {
    pub fn new<Child: 'static + Component + View<Child>>(
        props: Child::Properties,
        activators: Vec<LazyActivator<Model>>,
    ) -> Self {
        let constructor: Box<Constructor<Model>> = Box::new(ComponentState::build::<Child>);
        VComponent {
            parent: PhantomData,
            model_type: TypeId::of::<Child>(),
            props: AnyProps::new(props),
            constructor,
            activators,
        }
    }
}

pub trait PropTransform<Model: Component, From, To> {
    fn transform(&mut self, from: From) -> To;
}

impl<Model: Component, A> PropTransform<Model, A, A> for Vec<LazyActivator<Model>> {
    fn transform(&mut self, from: A) -> A {
        from
    }
}

impl<'a, Model: Component, A: Clone> PropTransform<Model, &'a A, A> for Vec<LazyActivator<Model>> {
    fn transform(&mut self, from: &'a A) -> A {
        from.clone()
    }
}

impl<'a, Model: Component> PropTransform<Model, &'a str, String> for Vec<LazyActivator<Model>> {
    fn transform(&mut self, from: &'a str) -> String {
        from.to_string()
    }
}

impl<'a, Model, F, A> PropTransform<Model, F, Option<Callback<A>>> for Vec<LazyActivator<Model>>
where
    Model: Component + View<Model> + 'static,
    F: Fn(A) -> Model::Message + 'static,
{
    fn transform(&mut self, from: F) -> Option<Callback<A>> {
        let cell = Rc::new(RefCell::new(None));
        self.push(cell.clone());
        let callback = move |arg| {
            let msg = from(arg);
            if let Some(ref mut sender) = *cell.borrow_mut() {
                sender.send_message(msg);
            } else {
                panic!("callback was not initialised by component")
            }
        };
        Some(callback.into())
    }
}
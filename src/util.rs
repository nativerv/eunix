use std::any::{Any, TypeId};

pub trait InstanceOf
where
    Self: Any,
{
    fn instance_of<U: ?Sized + Any>(&self) -> bool {
        TypeId::of::<Self>() == TypeId::of::<U>()
    }
}

// implement this trait for every type that implements `Any` (which is most types)
impl<T: ?Sized + Any> InstanceOf for T {}

pub unsafe fn any_as_u8_slice<T: Sized>(p: &T) -> &[u8] {
::std::slice::from_raw_parts(
    (p as *const T) as *const u8,
    ::std::mem::size_of::<T>(),
)
}

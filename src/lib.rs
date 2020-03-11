use futures_core::{
    future::Future,
    task::{self, Poll},
};
use std::pin::Pin;

#[doc(no_inline)]
pub use scoped_tls::{scoped_thread_local, ScopedKey};

pub trait ScopedKeyExt<T> {
    fn set_async<'a, Fut>(&'static self, t: &'a T, fut: Fut) -> SetAsync<'a, T, Fut>
    where
        T: 'static,
        Fut: Future;
}

impl<T> ScopedKeyExt<T> for ScopedKey<T> {
    fn set_async<'a, Fut>(&'static self, t: &'a T, fut: Fut) -> SetAsync<'a, T, Fut>
    where
        T: 'static,
        Fut: Future,
    {
        SetAsync { key: self, t, fut }
    }
}

#[must_use = "futures do nothing unless you `.await` or poll them"]
pub struct SetAsync<'a, T: 'static, Fut> {
    key: &'static ScopedKey<T>,
    t: &'a T,
    fut: Fut,
}

impl<T, Fut> Future for SetAsync<'_, T, Fut>
where
    Fut: Future,
{
    type Output = Fut::Output;

    #[inline]
    fn poll(self: Pin<&mut Self>, cx: &mut task::Context<'_>) -> Poll<Self::Output> {
        unsafe {
            let me = self.get_unchecked_mut();
            let key = me.key;
            let t = me.t;
            let fut = Pin::new_unchecked(&mut me.fut);
            key.set(t, || fut.poll(cx))
        }
    }
}

use core::future::Future;
use core::sync::atomic::{AtomicBool, Ordering};
use core::task::{Context, Poll};
use core::marker::PhantomData;
use futures::task::AtomicWaker;

pub trait StateCellRead {
    type Data;

    fn get(&self) -> Self::Data;
}
pub struct NoopStateCell;

impl StateCellRead for NoopStateCell {
    type Data = ();

    fn get(&self) {}
}

pub struct Notification {
    waker: AtomicWaker,
    triggered: AtomicBool,
}

impl Notification {
    pub const fn new() -> Self {
        Self {
            waker: AtomicWaker::new(),
            triggered: AtomicBool::new(false),
        }
    }

    pub fn notify(&self) {
        self.triggered.store(true, Ordering::SeqCst);
        self.waker.wake();
    }

    pub fn poll_wait(&self, cx: &mut Context<'_>) -> Poll<()> {
        self.waker.register(cx.waker());

        if self.triggered.swap(false, Ordering::SeqCst) {
            Poll::Ready(())
        } else {
            Poll::Pending
        }
    }

    pub fn wait(&self) -> impl Future<Output = ()> + '_ {
        futures::future::poll_fn(move |cx| self.poll_wait(cx))
    }
}

// used in NotifReceiver
pub trait Sender {
    type Data: Send;

    type SendFuture<'a>: Future
    where
        Self: 'a;

    fn send(&mut self, value: Self::Data) -> Self::SendFuture<'_>;
}
pub trait Receiver {
    type Data: Send;

    type RecvFuture<'a>: Future<Output = Self::Data>
    where
        Self: 'a;

    fn recv(&mut self) -> Self::RecvFuture<'_>;
}

pub struct NotifReceiver<'a, S>(&'a Notification, &'a S);

impl<'a, S> NotifReceiver<'a, S> {
    pub const fn new(notif: &'a Notification, state: &'a S) -> Self {
        Self(notif, state)
    }
}
impl<'a, S> Receiver for NotifReceiver<'a, S>
where
    S: StateCellRead + Send + Sync + 'a,
    S::Data: Send,
{
    type Data = S::Data;

    type RecvFuture<'b> = impl Future<Output = Self::Data> where Self: 'b;

    fn recv(&mut self) -> Self::RecvFuture<'_> {
        async move {
            self.0.wait().await;

            self.1.get()
        }
    }
}
impl<'a> Receiver for NotifReceiver<'a, ()> {
    type Data = ();

    type RecvFuture<'b> = impl Future<Output = ()> where Self: 'b;

    fn recv(&mut self) -> Self::RecvFuture<'_> {
        async move {
            self.0.wait().await;
        }
    }
}

pub struct NotifSender<'a, const N: usize, P = ()>(
    [&'a Notification; N],
    &'static str,
    PhantomData<fn() -> P>,
);

impl<'a, const N: usize, P> NotifSender<'a, N, P> {
    pub const fn new(source: &'static str, notif: [&'a Notification; N]) -> Self {
        Self(notif, source, PhantomData)
    }
}

impl<'a, const N: usize, P> Sender for NotifSender<'a, N, P>
where
    P: core::fmt::Debug + Send,
{
    type Data = P;

    type SendFuture<'b> = impl Future<Output = ()>
    where Self: 'b;

    fn send(&mut self, value: Self::Data) -> Self::SendFuture<'_> {
        async move {
            //info!("[{} SIGNAL]: {:?}", self.1, value);

            for notif in self.0 {
                notif.notify();
            }
        }
    }
}
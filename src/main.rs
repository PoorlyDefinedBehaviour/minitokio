use crossbeam::channel;
use futures::task::{self, ArcWake};
use std::sync::Mutex;
use std::{
  future::Future,
  pin::Pin,
  sync::Arc,
  task::{Context, Poll},
  thread,
  time::{Duration, Instant},
};

struct Delay {
  when: Instant,
}

impl Future for Delay {
  type Output = &'static str;

  fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<&'static str> {
    if Instant::now() >= self.when {
      println!("hello world");
      Poll::Ready("done")
    } else {
      let waker = cx.waker().clone();
      let when = self.when;

      // NOTE: would we use epoll here?
      thread::spawn(move || {
        let now = Instant::now();

        if now < when {
          thread::sleep(when - now);
        }

        waker.wake();
      });

      Poll::Pending
    }
  }
}
struct MiniTokio {
  scheduled: channel::Receiver<Arc<Task>>,
  sender: channel::Sender<Arc<Task>>,
}

struct Task {
  // The `Mutex` is to make `Task` implement `Sync`. Only
  // one thread accesses `future` at any given time. The
  // `Mutex` is not required for correctness. Real Tokio
  // does not use a mutex here, but real Tokio has
  // more lines of code than can fit in a single tutorial
  // page.
  future: Mutex<Pin<Box<dyn Future<Output = ()> + Send>>>,
  executor: channel::Sender<Arc<Task>>,
}

impl Task {
  fn schedule(self: &Arc<Self>) {
    self
      .executor
      .send(self.clone())
      .expect("unable to send message to executor channel");
  }

  fn poll(self: Arc<Self>) {
    let waker = task::waker(self.clone());
    let mut cx = Context::from_waker(&waker);

    let mut future = self.future.lock().unwrap();

    let _ = future.as_mut().poll(&mut cx);
  }

  fn spawn<F>(future: F, sender: &channel::Sender<Arc<Task>>)
  where
    F: Future<Output = ()> + Send + 'static,
  {
    let task = Arc::new(Task {
      future: Mutex::new(Box::pin(future)),
      executor: sender.clone(),
    });

    let _ = sender.send(task);
  }
}

impl ArcWake for Task {
  fn wake_by_ref(arc_self: &Arc<Self>) {
    arc_self.schedule();
  }
}

impl MiniTokio {
  fn new() -> Self {
    let (sender, receiver) = channel::unbounded();

    Self {
      scheduled: receiver,
      sender,
    }
  }

  fn spawn<F>(&mut self, future: F)
  where
    F: Future<Output = ()> + Send + 'static,
  {
    Task::spawn(future, &self.sender);
  }

  fn run(&mut self) {
    while let Ok(task) = self.scheduled.recv() {
      task.poll();
    }
  }
}

fn main() {
  let mut mini_tokio = MiniTokio::new();

  mini_tokio.spawn(async {
    let when = Instant::now() + Duration::from_millis(10);
    let future = Delay { when };

    let out = future.await;
    assert_eq!(out, "done");
  });

  mini_tokio.run();
}

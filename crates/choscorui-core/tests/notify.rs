use choscorui_core::{
    NotificationQueue, NotifyError, Point, Rect, Size, Tree, WorkError, WorkQueue,
};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

fn ms(value: u64) -> Duration {
    Duration::from_millis(value)
}

fn rect() -> Rect {
    Rect::new(Point::new(0.0, 0.0), Size::new(10.0, 10.0))
}

struct App {
    tree: Tree,
    log: Vec<String>,
}

#[test]
fn notifications_deliver_fifo_with_cascades_and_skip_removed_sources() {
    let mut tree = Tree::new(rect());
    let a = tree.attach(tree.root(), rect()).unwrap();
    let b = tree.attach(tree.root(), rect()).unwrap();
    let mut app = App {
        tree,
        log: Vec::new(),
    };
    let mut queue = NotificationQueue::new();
    queue.push(a, "a-changed");
    queue.push(b, "b-changed");
    let delivered = queue
        .drain(
            &mut app,
            |app| &app.tree,
            |app, queue, note| {
                app.log.push(note.kind.to_owned());
                if note.kind == "a-changed" {
                    // A handler removes b, then reports a follow-up on a.
                    app.tree.detach(b).unwrap();
                    queue.push(a, "a-follow-up");
                }
            },
        )
        .unwrap();
    assert_eq!(delivered, 2);
    assert_eq!(app.log, ["a-changed", "a-follow-up"]);
    assert!(queue.is_empty());
}

#[test]
fn notification_feedback_loop_fails_predictably() {
    let tree = Tree::new(rect());
    let root = tree.root();
    let mut app = App {
        tree,
        log: Vec::new(),
    };
    let mut queue = NotificationQueue::with_cascade_limit(5);
    queue.push(root, ());
    let result = queue.drain(
        &mut app,
        |app| &app.tree,
        |_, queue, note| queue.push(note.source, ()),
    );
    assert_eq!(result, Err(NotifyError::CascadeLimit));
    assert!(queue.is_empty());
}

#[test]
fn posted_work_runs_before_due_timers_in_deadline_then_creation_order() {
    let mut queue: WorkQueue<Vec<&'static str>> = WorkQueue::new();
    queue.schedule_once(ms(0), ms(20), |log| log.push("t20"));
    queue.schedule_once(ms(0), ms(10), |log| log.push("t10-first"));
    queue.schedule_once(ms(0), ms(10), |log| log.push("t10-second"));
    queue.sender().post(|log| log.push("posted")).unwrap();
    let mut log = Vec::new();
    assert_eq!(queue.run_ready(ms(5), &mut log), 1);
    assert_eq!(queue.next_deadline(), Some(ms(10)));
    assert_eq!(queue.run_ready(ms(30), &mut log), 3);
    assert_eq!(log, ["posted", "t10-first", "t10-second", "t20"]);
    assert_eq!(queue.next_deadline(), None);
}

#[test]
fn canceled_timer_never_runs_and_repeating_timer_skips_missed_ticks() {
    let mut queue: WorkQueue<u32> = WorkQueue::new();
    let once = queue.schedule_once(ms(0), ms(10), |count| *count += 100);
    let repeat = queue
        .schedule_repeating(ms(0), ms(10), |count| *count += 1)
        .unwrap();
    assert!(queue.cancel(once));
    assert!(!queue.cancel(once));
    let mut count = 0;
    queue.run_ready(ms(10), &mut count);
    assert_eq!(count, 1);
    // Thirty-five milliseconds late: one tick runs, the next is at 50.
    queue.run_ready(ms(45), &mut count);
    assert_eq!(count, 2);
    assert_eq!(queue.next_deadline(), Some(ms(50)));
    assert!(queue.is_scheduled(repeat));
    assert!(queue.cancel(repeat));
    queue.run_ready(ms(100), &mut count);
    assert_eq!(count, 2);
    assert_eq!(
        queue
            .schedule_repeating(ms(0), Duration::ZERO, |_| {})
            .err(),
        Some(WorkError::InvalidInterval)
    );
}

#[test]
fn workers_post_in_order_and_wake_the_ui_thread_without_mutating_state() {
    let mut queue: WorkQueue<Vec<u32>> = WorkQueue::new();
    let wakes = Arc::new(AtomicUsize::new(0));
    let counter = Arc::clone(&wakes);
    queue.set_waker(move || {
        counter.fetch_add(1, Ordering::SeqCst);
    });
    let sender = queue.sender();
    std::thread::spawn(move || {
        for value in 0..50 {
            sender.post(move |log| log.push(value)).unwrap();
        }
    })
    .join()
    .unwrap();
    assert_eq!(wakes.load(Ordering::SeqCst), 50);
    assert!(queue.has_posted_work());
    let mut log = Vec::new();
    queue.run_ready(ms(0), &mut log);
    assert_eq!(log, (0..50).collect::<Vec<_>>());
}

#[test]
fn work_posted_while_running_is_deferred_and_shutdown_rejects_posts() {
    let mut queue: WorkQueue<Vec<&'static str>> = WorkQueue::new();
    let sender = queue.sender();
    let inner = sender.clone();
    sender
        .post(move |log| {
            log.push("outer");
            inner.post(|log| log.push("inner")).unwrap();
        })
        .unwrap();
    let mut log = Vec::new();
    assert_eq!(queue.run_ready(ms(0), &mut log), 1);
    assert_eq!(log, ["outer"]);
    assert_eq!(queue.run_ready(ms(0), &mut log), 1);
    assert_eq!(log, ["outer", "inner"]);
    queue.schedule_once(ms(0), ms(1), |log| log.push("never"));
    queue.shutdown();
    assert_eq!(sender.post(|_| {}), Err(WorkError::Closed));
    assert_eq!(queue.run_ready(ms(10), &mut log), 0);
}

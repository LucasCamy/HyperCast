//! Finish media teardown away from the UI thread, before admitting another room.
use std::sync::mpsc;

pub struct Departure { done:mpsc::Receiver<()>,worker:Option<std::thread::JoinHandle<()>> }
impl Departure {
    pub fn start<T:Send+'static>(engine:Option<T>,ctx:eframe::egui::Context)->Self {
        let(tx,done)=mpsc::channel();
        let worker=std::thread::spawn(move||{drop(engine);let _=tx.send(());ctx.request_repaint();});
        Self{done,worker:Some(worker)}
    }
    pub fn ready(&self)->bool {
        !matches!(self.done.try_recv(),Err(mpsc::TryRecvError::Empty))
    }
}

impl Drop for Departure {
    fn drop(&mut self){if let Some(worker)=self.worker.take(){let _=worker.join();}}
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn next_room_waits_for_teardown_without_blocking_the_ui() {
        struct SlowEngine(mpsc::Receiver<()>,mpsc::Sender<()>);
        impl Drop for SlowEngine {fn drop(&mut self){let _=self.1.send(());let _=self.0.recv();}}
        let(release,wait)=mpsc::channel();let(started,received)=mpsc::channel();
        let departure=Departure::start(Some(SlowEngine(wait,started)),Default::default());
        received.recv_timeout(std::time::Duration::from_secs(2)).unwrap();
        assert!(!departure.ready());
        release.send(()).unwrap();
        let until=std::time::Instant::now()+std::time::Duration::from_secs(2);
        while !departure.ready(){assert!(std::time::Instant::now()<until);std::thread::yield_now();}
    }
}

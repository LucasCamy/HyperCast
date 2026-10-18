//! Finish media teardown away from the UI thread, before admitting another room.
use std::sync::mpsc;

/// Independent of the command queue, so exit cannot sit behind media work.
#[derive(Clone)]
pub struct Cancellation(tokio::sync::watch::Sender<bool>);
impl Default for Cancellation {fn default()->Self {Self(tokio::sync::watch::channel(false).0)}}
impl Cancellation {
    pub fn cancel(&self){self.0.send_replace(true);}
    pub fn is_cancelled(&self)->bool {*self.0.borrow()}
    pub async fn cancelled(&self){let mut rx=self.0.subscribe();if *rx.borrow_and_update(){return;}let _=rx.changed().await;}
    pub async fn run<T>(&self,work:impl std::future::Future<Output=T>)->Option<T>{
        tokio::select!{biased;_=self.cancelled()=>None,value=work=>Some(value)}
    }
}

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

#[cfg(test)]
mod cancellation_tests {
    use super::*;
    use std::time::{Duration,Instant};
    #[test]
    fn cancellation_is_sticky_and_beats_ready_work(){
        let runtime=tokio::runtime::Runtime::new().unwrap();let cancel=Cancellation::default();cancel.cancel();
        runtime.block_on(async{assert_eq!(cancel.run(async{7}).await,None);tokio::time::timeout(Duration::from_millis(100),cancel.cancelled()).await.unwrap();});
    }
    #[test]
    fn closing_during_websocket_handshake_does_not_wait_for_connection_timeout(){
        for _ in 0..3 {
            let listener=std::net::TcpListener::bind("127.0.0.1:0").unwrap();let address=format!("ws://{}",listener.local_addr().unwrap());
            let (tx,rx)=mpsc::channel();let (release,wait)=mpsc::channel();
            let server=std::thread::spawn(move||{let (_socket,_)=listener.accept().unwrap();tx.send(()).unwrap();let _=wait.recv_timeout(Duration::from_secs(5));});
            #[cfg(windows)]
            let engine=crate::desktop_engine::Engine::connect(address,"test".into(),Default::default(),0,false);
            #[cfg(target_os="linux")]
            let engine=crate::linux::engine::Engine::start(address,"test".into(),Default::default());
            rx.recv_timeout(Duration::from_secs(3)).unwrap();let started=Instant::now();drop(engine);let elapsed=started.elapsed();
            let _=release.send(());server.join().unwrap();assert!(elapsed<Duration::from_secs(2),"Exit waited for connection: {elapsed:?}");
        }
    }
    #[test]
    #[ignore="Requires a scoped token for an isolated test room"]
    fn cancel_pending_room_connections(){
        let address=std::env::var("HYPERCAST_URL").unwrap();let token=std::env::var("HYPERCAST_ROOM_TOKEN").unwrap();
        for delay in [0,5,30,60,120,300,5,60,300] {
            #[cfg(windows)]
            let engine=crate::desktop_engine::Engine::connect(address.clone(),token.clone(),Default::default(),0,false);
            #[cfg(target_os="linux")]
            let engine=crate::linux::engine::Engine::start(address.clone(),token.clone(),Default::default());
            std::thread::sleep(Duration::from_millis(delay));let now=Instant::now();drop(engine);
            assert!(now.elapsed()<Duration::from_secs(3),"Pending room cancellation stalled after {delay} ms");
        }
    }
    #[test]
    fn closing_during_http_admission_discards_late_results(){
        let listener=std::net::TcpListener::bind("127.0.0.1:0").unwrap();let address=format!("ws://{}",listener.local_addr().unwrap());
        let (tx,rx)=mpsc::channel();let (release,wait)=mpsc::channel();
        let server=std::thread::spawn(move||{let (_socket,_)=listener.accept().unwrap();tx.send(()).unwrap();let _=wait.recv_timeout(Duration::from_secs(5));});
        let job=crate::invitation::JoinAttempt::start(address,"room".into(),"Test".into(),"test".into(),"test".into(),true,Default::default());
        rx.recv_timeout(Duration::from_secs(3)).unwrap();let started=Instant::now();drop(job);let elapsed=started.elapsed();
        let _=release.send(());server.join().unwrap();assert!(elapsed<Duration::from_secs(2),"Exit waited for HTTP: {elapsed:?}");
    }
}

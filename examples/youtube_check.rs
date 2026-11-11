//! Hidden check of the in-process YouTube player: an invisible Rune window, a silent player (no
//! window, throwaway session, muted media, no audio tap), a song page; waits for the queue,
//! plays its third song and confirms the title moves on. Exits 0 on success.
//! `cargo run --release --example youtube_check`

#[path = "../src/youtube.rs"]
#[allow(dead_code)]
mod youtube;

use rune::{AppSpec, Cx, El, WindowOptions, el};
use std::time::{Duration, Instant};
use youtube::{Options, YouTube};

#[derive(Default)]
struct S {
    yt: Option<YouTube>,
    since: Option<Instant>,
    first: String,
    clicked: Option<String>,
}

fn view(_: &S, _: &mut Cx<S>) -> El {
    el()
}

fn done(ok: bool, msg: String) -> ! {
    println!(
        "{} {msg}",
        if ok { "YOUTUBE OK:" } else { "YOUTUBE FAILED:" }
    );
    std::process::exit(if ok { 0 } else { 1 });
}

fn main() {
    rune::App::new(AppSpec::<S> {
        setup: Some(|s, waker| {
            let options = Options {
                silent: true,
                tap: false,
                url: Some("https://music.youtube.com/watch?v=kJQP7kiw5Fk".into()),
            };
            s.yt = Some(YouTube::start(waker, &options));
            s.since = Some(Instant::now());
        }),
        poll: Some(|s, _, tx| {
            tx.wake_after(Duration::from_millis(200));
            let yt = s.yt.as_mut().unwrap();
            yt.poll();
            let elapsed = s.since.unwrap().elapsed();
            let Some(st) = yt.status().cloned() else {
                if elapsed > Duration::from_secs(30) {
                    done(false, "no status in 30 s".into());
                }
                return false;
            };
            match &s.clicked {
                None if st.queue.len() > 3 => {
                    s.first = st.title.clone();
                    let target = st.queue[2].title.clone();
                    println!(
                        "now {:?}, tap {:?}, queue {} songs; playing queue[2] {:?}",
                        st.title,
                        st.tap,
                        st.queue.len(),
                        target
                    );
                    yt.send("queue 2");
                    s.clicked = Some(target);
                    s.since = Some(Instant::now());
                }
                None if elapsed > Duration::from_secs(30) => {
                    done(false, format!("no queue (page title {:?})", st.title))
                }
                Some(target) => {
                    // YouTube may play an ad first, and its now-playing title can be shorter
                    // than the queue's ("Bailando" for "Bailando (feat. ...)").
                    let base = target.split(" (").next().unwrap_or(target);
                    if st.title.starts_with(base) {
                        done(true, format!("{:?} → {:?}", s.first, st.title));
                    }
                    if elapsed > Duration::from_secs(45) {
                        done(false, format!("still {:?} 45 s after the click", st.title));
                    }
                }
                _ => {}
            }
            false
        }),
        ..AppSpec::new("YouTube check", (200.0, 120.0), S::default, view)
    })
    .window_options(WindowOptions {
        visible: false,
        ..WindowOptions::default()
    })
    .run();
}

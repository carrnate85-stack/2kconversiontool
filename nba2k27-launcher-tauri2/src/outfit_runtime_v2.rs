//! V2 implementation child of outfit_runtime: deliberately reuses the exact
//! authenticated Queue/Command/Status types without exposing their internals.
//! Root wires only #[path = "outfit_runtime_v2.rs"] pub(crate) mod v2; in its parent.
#![allow(dead_code)]
use super::*;
pub(crate) struct Native {
    stop: Arc<AtomicBool>,
    worker: Option<JoinHandle<Result<()>>>,
    queue: Arc<Queue>,
    terminal_error: Option<String>,
}
impl Native {
    pub fn start(
        owner: Owner,
        queue: Arc<Queue>,
        admission: crate::outfit_worker_v2_lease::Admission,
    ) -> Result<Self> {
        admission.check(&owner.root, &owner.session, &owner.identity, &owner.parent)?;
        let stop = Arc::new(AtomicBool::new(false));
        let ended = stop.clone();
        let q = queue.clone();
        let worker = thread::Builder::new()
            .name("native-outfit-v2-session".into())
            .spawn(move || run(owner, q, ended, admission))?;
        Ok(Self {
            stop,
            worker: Some(worker),
            queue,
            terminal_error: None,
        })
    }
    pub fn ended(&mut self, closed: impl Fn() -> Result<()>) -> Result<()> {
        closed()?;
        self.stop.store(true, Ordering::Release);
        self.queue.wake_worker();
        if let Some(e) = &self.terminal_error {
            anyhow::bail!("{e}")
        }
        if let Some(worker) = self.worker.take() {
            let result = worker
                .join()
                .map_err(|_| {
                    anyhow::anyhow!("Native outfit worker failed; retain its ownership receipt")
                })
                .and_then(|r| r);
            if let Err(e) = result {
                self.terminal_error = Some(format!("{e:#}"));
                return Err(e);
            }
        }
        let s = self.queue.view()?;
        self.queue.publish(
            s.revision,
            "ended",
            "Game ended; outfit session released.",
            false,
            "ended",
            None,
            true,
        )
    }
}
/// The worker DLL silently refuses to run while another launcher-named
/// process exists (outfit_match_v2_proxy_bridge.hpp protected_processes).
/// Name them so an unarmed worker is diagnosable from the overlay.
fn launcher_blockers(owner: &Owner) -> String {
    let Ok(processes) = crate::session_safety::processes() else {
        return String::new();
    };
    let others = processes
        .iter()
        .filter(|p| blocks_worker(p, std::process::id(), &owner.parent))
        .map(|p| format!("{} (PID {})", p.name, p.pid))
        .collect::<Vec<_>>();
    if others.is_empty() {
        String::new()
    } else {
        format!(
            ". Other launcher processes stop the native worker: {}; close them and restart the game",
            others.join(", ")
        )
    }
}
fn blocks_worker(p: &crate::session_safety::Process, worker: u32, parent: &Identity) -> bool {
    p.pid != worker && p.identity.as_ref() != Some(parent) && worker_blocking_name(&p.name)
}
/// Same names as the DLL's launcher_name().
pub(crate) fn worker_blocking_name(name: &str) -> bool {
    let n = name.to_ascii_lowercase();
    [
        "2k launcher.exe",
        "2k launcher wpf.exe",
        "2k launcher rust.exe",
        "2k launcher rust prototype.exe",
        "2k launcher tauri qa.exe",
        "nba2k27-launcher-tauri2.exe",
    ]
    .contains(&n.as_str())
        || (n.len() > 16 && n.starts_with("2k-launcher-") && n.ends_with(".exe"))
}
fn run(
    owner: Owner,
    queue: Arc<Queue>,
    stop: Arc<AtomicBool>,
    admission: crate::outfit_worker_v2_lease::Admission,
) -> Result<()> {
    queue.enable_automatic_choice_protection()?;
    // Opt-in diagnostics has a separate read-only handle/thread and no error path
    // into this feature. No effect capability is shared with the monitor.
    #[cfg(debug_assertions)]
    let shadow = crate::outfit_match_v2::shadow::Monitor::start(owner.clone(), stop.clone());
    let clock = Instant::now();
    // Immutable launcher-side data: prepare it on this background worker when
    // outfits start, so the first selected-player reply need not decompress
    // and parse the catalog. This does not read or write game memory.
    let _ = crate::outfits::catalog();
    let mut session: Option<
        crate::outfit_carry_v2::Service<crate::blacktop_native::outfit_selection_v2_writer::Writer>,
    > = None;
    let mut active_revision = 0u64;
    let mut last_state = String::new();
    while !stop.load(Ordering::Acquire) {
        if queue.view()?.phase != "held" {
            if let Err(e) =
                admission.check(&owner.root, &owner.session, &owner.identity, &owner.parent)
            {
                queue.fail(&format!("V2 outfit launch admission ended: {e:#}"))?;
            }
        }
        if queue.view()?.phase == "held" {
            #[cfg(debug_assertions)]
            if let Some(shadow) = &shadow {
                shadow.disable();
            }
            queue.wait_for_work(&stop, Duration::from_millis(100))?;
            continue;
        }
        if let Some(request) = queue.take()? {
            active_revision = request.revision;
            last_state.clear();
            let result = (|| -> Result<()> {
                admission.check(&owner.root, &owner.session, &owner.identity, &owner.parent)?;
                ensure!(
                    request.created.elapsed() <= Duration::from_secs(3),
                    "Outfit request expired before native execution; refresh players"
                );
                if session.is_none() {
                    let reader = crate::blacktop_native::outfit_selection_v2_writer::Writer::open(
                        &owner.identity,
                        &owner.game,
                        &owner.parent,
                    )?;
                    session = Some(crate::outfit_carry_v2::Service::create(
                        &owner.root,
                        &owner.session,
                        reader,
                    )?);
                }
                let s = session.as_mut().unwrap();
                let now = clock.elapsed().as_millis() as u64;
                // Each mutating Service method mints and revalidates its own
                // fresh single-use player tokens. Do not perform an unused
                // extra List before that guarded capture (especially Random All).
                match request.command {
                    Command::List {} => queue.publish(
                        active_revision,
                        "ready",
                        "Native Blacktop players refreshed.",
                        false,
                        "selection",
                        Some(s.list(now)?),
                        true,
                    )?,
                    Command::Apply {
                        player_binding,
                        outfit,
                    } => {
                        s.apply(&player_binding, &outfit, clock.elapsed().as_millis() as u64)?;
                        queue.publish(
                            active_revision,
                            "refreshing",
                            "Waiting for the selected player's clothing refresh.",
                            true,
                            "selection",
                            None,
                            false,
                        )?;
                    }
                    Command::Restore { player_binding } => {
                        s.restore(&player_binding, clock.elapsed().as_millis() as u64)?;
                        queue.publish(
                            active_revision,
                            "refreshing",
                            "Restoring the selected player's original outfit.",
                            true,
                            "selection",
                            None,
                            false,
                        )?;
                    }
                    Command::ApplyMany { items, automatic } => {
                        let items = items
                            .into_iter()
                            .map(|i| (i.player_binding, i.outfit))
                            .collect::<Vec<_>>();
                        let applied = s.apply_many_with_origin(
                            &items,
                            clock.elapsed().as_millis() as u64,
                            automatic,
                        )?;
                        if applied == 0 {
                            queue.publish(active_revision, "ready", "Automatic outfits already verified and queued for this match; no new write.", false, "selection", s.list(clock.elapsed().as_millis() as u64).ok(), true)?;
                            return Ok(());
                        }
                        s.mark_batch_written(clock.elapsed().as_millis() as u64);
                        queue.publish(
                            active_revision,
                            "refreshing",
                            &format!("Waiting for {applied} players' clothing refresh."),
                            true,
                            "selection",
                            None,
                            false,
                        )?;
                    }
                }
                Ok(())
            })();
            if let Err(e) = result {
                let held = session.as_ref().is_some_and(|s| s.held().is_some())
                    || outfit_journal::pending(&owner.root)? && session.is_none();
                queue.publish(
                    active_revision,
                    if held { "held" } else { "error" },
                    &format!("{e:#}"),
                    false,
                    "unknown",
                    None,
                    true,
                )?;
            }
        }
        if let Some(s) = &mut session {
            if queue.view()?.phase != "held" {
                let result = s.tick(clock.elapsed().as_millis() as u64);
                match result {
                    Ok(value) => {
                        use outfit_journal::runtime::Status as P;
                        let key = serde_json::to_string(&value)?;
                        if key != last_state {
                            last_state = key;
                            // Snapshot only when the existing status changes;
                            // no extra per-poll diagnostics I/O or game reads.
                            queue.record_carry_timing(s.timing())?;
                            let current = queue.view()?;
                            match value{
                                P::SelectionUpdated{slot,name,carry_over}=>{
                                    let view=s.list(clock.elapsed().as_millis()as u64).ok();
                                    queue.publish(active_revision,"ready",&format!("{name} · slot {} · {}",slot+1,if carry_over{"outfit verified; queued for this Blacktop match"}else{"selection verified; no match carry-over queued (restore or two-piece outfit)"}),false,"selection",view,true)?;
                                },
                                P::SelectionUpdatedMany{players,timing}=>{
                                    let view=s.list(clock.elapsed().as_millis()as u64).ok();
                                    let queued=players.iter().filter(|p|p.carry_over).count();
                                    let names=players.iter().map(|p|format!("{}. {}",p.slot+1,p.name)).collect::<Vec<_>>().join(", ");
                                    queue.publish(active_revision,"ready",&format!("{} outfits verified ({queued} queued for this Blacktop match): {names} · write {} ms, game refresh {} ms",players.len(),timing.write_ms,timing.refresh_ms),false,"selection",view,true)?;
                                },
                                P::SelectionRefreshing=>{},
                                P::Applying{number,rebuilt}=>queue.publish(active_revision,"applying",&format!("Rebuilding Blacktop match {number} outfits · {rebuilt} confirmed"),true,"match",None,false)?,
                                P::Applied{number,players}=>queue.publish(active_revision,"ready",&format!("Blacktop match {number} · {players} outfit(s) rebuilt; gameplay verification remains required."),false,"match",None,true)?,
                                P::Menu=>queue.publish(active_revision,"waiting-for-selection","Return to Blacktop player selection to edit outfits.",false,"menu",None,true)?,
                                P::Rearming{..}=>{
                                    let selection=s.observed_boundary()==outfit_journal::runtime::Boundary::Selection;
                                    queue.publish(active_revision,"rearming","Waiting for stable Blacktop selection and the native outfit worker.",false,if selection{"selection"}else{"unknown"},if selection{current.selection}else{None},true)?;
                                },
                                P::Selecting if current.boundary!="selection"=>queue.publish(active_revision,"ready","Blacktop selection ready; refresh the player list.",false,"selection",None,true)?,
                                P::NoChoices=>queue.publish(active_revision,"waiting-for-selection","No new outfit choices for this match. Edit outfits at Blacktop player selection.",false,"match",None,true)?,
                                P::WaitingForMatch if current.boundary=="selection"=>queue.publish(active_revision,"waiting-for-match","Blacktop is transitioning; player editing waits for selection.",false,"unknown",None,true)?,
                                P::WaitingForWorker{number}=>queue.publish(active_revision,"waiting-for-worker",&format!("Waiting for native outfit worker {number}."),false,"match",None,false)?,
                                P::NotReady{reason}=>{
                                    let blockers=if reason.contains("has not armed"){launcher_blockers(&owner)}else{String::new()};
                                    queue.publish(active_revision,"waiting-for-match",&format!("Blacktop match outfits not carried yet: {reason}{blockers}"),false,"match",None,true)?
                                },
                                P::WaitingForMatch|P::Selecting=>{},
                            }
                        }
                    }
                    Err(e) => {
                        queue.record_carry_timing(s.timing())?;
                        queue.publish(
                            active_revision,
                            "held",
                            &format!("Outfit session stopped without retry: {e:#}"),
                            false,
                            "unknown",
                            None,
                            true,
                        )?;
                    }
                }
            }
        }
        #[cfg(debug_assertions)]
        if let Some(shadow) = &shadow {
            if queue
                .view()
                .ok()
                .is_none_or(|status| status.phase == "held")
            {
                shadow.disable();
            } else {
                shadow.offer(
                    session
                        .as_ref()
                        .map_or_else(Vec::new, |session| session.shadow_choices()),
                );
            }
        }
        // Tick faster only while a selection refresh is pending confirmation.
        let pending = session.as_ref().is_some_and(|s| s.busy());
        queue.wait_for_work(&stop, Duration::from_millis(if pending { 20 } else { 100 }))?;
    }
    let closed = || crate::provider_launch::closed(&owner.parent);
    if let Some(s) = &mut session {
        s.finish_ended(&closed)?;
    } else if outfit_journal::pending(&owner.root)? {
        outfit_journal::resolve_ended(&owner.root, &owner.session, &owner.identity, &closed)?;
    }
    Ok(())
}

impl Drop for Native {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        self.queue.wake_worker();
    }
}
#[cfg(test)]
mod ownership_diagnostic_tests {
    use super::*;
    #[test]
    fn owning_ui_is_exempt_but_reused_or_unknown_pid_is_not() {
        let parent = Identity {
            pid: 300,
            started: "1000".into(),
            path: r"C:\Fixture\App\2k Launcher Rust Prototype.exe".into(),
        };
        let row = |identity| crate::session_safety::Process {
            name: "2k Launcher Rust Prototype.exe".into(),
            pid: 300,
            parent: 1,
            identity,
        };
        assert!(!blocks_worker(&row(Some(parent.clone())), 200, &parent));
        assert!(blocks_worker(&row(None), 200, &parent));
        let mut reused = parent.clone();
        reused.started = "2000".into();
        assert!(blocks_worker(&row(Some(reused)), 200, &parent));
        assert!(!blocks_worker(&row(None), 300, &parent));
    }
}

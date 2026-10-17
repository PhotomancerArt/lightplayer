//! EXPERIMENT (exp/c6-flash-mode): write 96 × 4 KiB of incompressible files
//! over the link, then power-cycle CX1 `LP_READBACK_BOOTS` times (default 3)
//! and read every file back after each power-on boot, comparing bytes. The
//! store reads `lpfs` through the flash cache (#1161), so this reads through
//! whatever SPI0 mode the firmware under test runs. Assumes the board is
//! already flashed with it. Env: `BOARD_HOLDER`, `LP_READBACK_LABEL`.

#[path = "support/desk_board.rs"]
mod desk_board;

use std::time::{Duration, Instant};

use desk_board::{Console, Desk, close, power_up};
use lpa_client::LpClient;
use lpfs::LpPath;

const FILES: u32 = 96;

fn data(i: u32) -> Vec<u8> {
    (0..4_096u32)
        .map(|j| ((j + i * 7_919 + 12_345).wrapping_mul(2_654_435_761) >> 24) as u8)
        .collect()
}

#[test]
#[ignore = "drives the sacrificial CX1 on the desk (module docs)"]
fn flash_readback_on_cx1() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("a runtime");
    let local = tokio::task::LocalSet::new();
    runtime.block_on(local.run_until(run()));
}

async fn run() {
    let label = std::env::var("LP_READBACK_LABEL").unwrap_or_else(|_| "?".into());
    let boots: usize = std::env::var("LP_READBACK_BOOTS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(3);
    let holder = std::env::var("BOARD_HOLDER").expect("BOARD_HOLDER");
    let desk = Desk::cx1(&holder);
    desk.check().unwrap_or_else(|e| panic!("{e}"));
    desk.renew(60).ok();
    let console = Console::new(Instant::now());
    let port = desk.port_wait(Duration::from_secs(30)).expect("CX1's port");
    let (conn, _) = power_up(&desk, &port, None, &console, &format!("{label} prep")).await;
    {
        let mut client = LpClient::new(conn.client_io());
        let mut written = 0;
        for i in 0..FILES {
            match client
                .fs_write(LpPath::new(&format!("/rb/f{i:03}.bin")), data(i))
                .await
            {
                Ok(_) => written += 1,
                Err(e) => panic!("{label}: write {i}: {e}"),
            }
        }
        eprintln!("{label}: wrote {written} × 4 KiB");
    }
    let mut conn = Some(conn);
    let mut bad = 0usize;
    for b in 0..boots {
        let (c, m) = power_up(&desk, &port, conn.take(), &console, &format!("{label} rb #{b}")).await;
        let mut client = LpClient::new(c.client_io());
        let t = Instant::now();
        let mut ok = 0;
        for i in 0..FILES {
            match client.fs_read(LpPath::new(&format!("/rb/f{i:03}.bin"))).await {
                Ok(o) if o.value == data(i) => ok += 1,
                Ok(o) => {
                    bad += 1;
                    eprintln!("{label} #{b}: f{i:03} differs ({} B)", o.value.len());
                }
                Err(e) => {
                    bad += 1;
                    eprintln!("{label} #{b}: f{i:03} read error: {e}");
                }
            }
        }
        drop(client);
        eprintln!(
            "== {label} readback #{b}: {ok}/{FILES} identical in {:.1} s, mount {:?} us",
            t.elapsed().as_secs_f64(),
            m["mount_us"]
        );
        conn = Some(c);
    }
    if let Some(c) = conn {
        // Leave the store as it was found, less these files.
        let mut client = LpClient::new(c.client_io());
        for i in 0..FILES {
            let _ = client
                .fs_delete_file(LpPath::new(&format!("/rb/f{i:03}.bin")))
                .await;
        }
        drop(client);
        close(c).await;
    }
    let crc_lines: Vec<_> = console
        .take()
        .into_iter()
        .filter(|(_, l)| l.contains("crc") || l.contains("CRC") || l.contains("corrupt"))
        .collect();
    eprintln!("== {label}: {bad} bad reads; console crc/corrupt lines: {crc_lines:?}");
    assert_eq!(bad, 0, "{label}: read-back mismatches");
}

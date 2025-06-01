use std::error::Error;
use std::time::Duration;

use bluest::{Adapter, Uuid};
use futures_lite::{future, StreamExt};
use tracing::metadata::LevelFilter;
use tracing::{error, info};

const GZUT_PIANO_MIDI_SERVICE: Uuid = Uuid::from_u128(0x03b80e5a_ede8_4b33_a751_6ce34ec4c700);
const GZUT_PIANO_MIDI_CHARACTERISTIC: Uuid = Uuid::from_u128(0x7772e5db_3868_4112_a1a9_f2669d106bf3);

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    use tracing_subscriber::prelude::*;
    use tracing_subscriber::{fmt, EnvFilter};

    tracing_subscriber::registry()
        .with(fmt::layer())
        .with(
            EnvFilter::builder()
                .with_default_directive(LevelFilter::INFO.into())
                .from_env_lossy(),
        )
        .init();

    let adapter = Adapter::default().await.ok_or("Bluetooth adapter not found")?;
    adapter.wait_available().await?;

    info!("looking for device");
    let device = adapter
        .discover_devices(&[GZUT_PIANO_MIDI_SERVICE])
        .await?
        .next()
        .await
        .ok_or("Failed to discover device")??;
    info!(
        "found device: {} ({:?})",
        device.name().as_deref().unwrap_or("(unknown)"),
        device.id()
    );

    adapter.connect_device(&device).await?;
    info!("connected!");

    let service = match device
        .discover_services_with_uuid(GZUT_PIANO_MIDI_SERVICE)
        .await?
        .first()
    {
        Some(service) => service.clone(),
        None => return Err("service not found".into()),
    };
    info!("found Piano midi service");

    let characteristics = service.characteristics().await?;
    info!("discovered characteristics");

    let midi_characteristic = characteristics
        .iter()
        .find(|x| x.uuid() == GZUT_PIANO_MIDI_CHARACTERISTIC)
        .ok_or("midi characteristic not found")?;

    let midi_fut = async {
        info!("enabling midi notifications");
        let mut updates = midi_characteristic.notify().await?;

        info!("waiting for midi changes");
        while let Some(val) = updates.next().await {
            info!("Midi state changed: {:?}", val?);
        }
        info!("finished waiting for midi changes");
        Ok(())
    };

    let led_characteristic = characteristics
        .iter()
        .find(|x| x.uuid() == GZUT_PIANO_MIDI_CHARACTERISTIC)
        .ok_or("led characteristic not found")?;

    let blink_fut = async {
        info!("blinking LED");
        tokio::time::sleep(Duration::from_secs(1)).await;
        loop {
            // led_characteristic.write(&[0x01]).await?;
            // info!("LED on");
            tokio::time::sleep(Duration::from_secs(1)).await;
            // led_characteristic.write(&[0x00]).await?;
            // info!("LED off");
            tokio::time::sleep(Duration::from_secs(1)).await;
        }
    };

    type R = Result<(), Box<dyn Error>>;
    let midi_fut = async move {
        let res: R = midi_fut.await;
        error!("Midi task exited: {:?}", res);
    };
    let blink_fut = async move {
        let res: R = blink_fut.await;
        error!("Blink task exited: {:?}", res);
    };

    future::zip(blink_fut, midi_fut).await;

    Ok(())
}

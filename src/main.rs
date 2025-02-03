use anyhow::Context;
use clap::{Parser, Subcommand};
use hex::encode;
use sev::firmware::guest::{AttestationReport, Firmware};
use std::{
    fs::{self, File},
    io::Write,
    path::PathBuf,
};

#[derive(Parser)]
#[command(name = "sev-tool")]
#[command(about = "AMD SEV management tool")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    FetchVcek {
        #[arg(short, long, default_value = "certs/VCEK.bin")]
        output: String,
    },
    Report,
}

async fn request_vcek(
    chip_id: [u8; 64],
    reported_tcb: sev::firmware::host::TcbVersion,
) -> anyhow::Result<Vec<u8>> {
    const KDS_CERT_SITE: &str = "https://kdsintf.amd.com";
    const KDS_VCEK: &str = "/vcek/v1";
    let hw_id: String = encode(&chip_id);

    let vcek_url = format!(
        "{KDS_CERT_SITE}{KDS_VCEK}/Genoa/\
        {hw_id}?blSPL={:02}&teeSPL={:02}&snpSPL={:02}&ucodeSPL={:02}",
        reported_tcb.bootloader, reported_tcb.tee, reported_tcb.snp, reported_tcb.microcode
    );

    loop {
        let response = reqwest::get(&vcek_url)
            .await
            .context("Failed to get VCEK from URL");

        match response {
            Ok(response) => {
                if response.status() == 429 {
                    println!("Received 429, sleeping for 10 seconds");
                    std::thread::sleep(std::time::Duration::from_secs(10));
                    continue;
                }
                let rsp_bytes = response.bytes().await?.to_vec();
                return Ok(rsp_bytes);
            }
            Err(e) => return Err(e.into()),
        }
    }
}

async fn fetch_vcek(output_path: &str) -> anyhow::Result<()> {
    let unique_data = [0u8; 64];

    let mut fw = Firmware::open().context("Failed to open firmware")?;

    let report = fw
        .get_report(None, Some(unique_data), None)
        .context("Failed to get attestation report")?;

    let vcek = request_vcek(report.chip_id, report.reported_tcb)
        .await
        .context("Failed to fetch VCEK")?;

    if let Some(parent) = PathBuf::from(output_path).parent() {
        fs::create_dir_all(parent)?;
    }

    let mut file = File::create(output_path)?;
    file.write_all(&vcek)?;

    println!("VCEK certificate saved to {}", output_path);
    Ok(())
}

fn display_report() -> anyhow::Result<()> {
    let unique_data = [0u8; 64];
    env_logger::builder().format_timestamp(None).init();

    let mut fw = Firmware::open().context("Failed to open firmware")?;

    log::info!("Opened firmware interface");

    let report: AttestationReport = fw
        .get_report(None, Some(unique_data), None)
        .context("Failed to get attestation report")?;

    println!("{:#?}", report);

    Ok(())
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::FetchVcek { output } => {
            fetch_vcek(&output).await?;
        }
        Commands::Report => {
            display_report()?;
        }
    }

    Ok(())
}

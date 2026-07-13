#[tokio::main]
async fn main() -> anyhow::Result<()> {
    alan_os_host::run_host_process("stable").await
}

use jsonrpsee::core::RpcResult;
use jsonrpsee::proc_macros::rpc;
use jsonrpsee::server::ServerBuilder;

#[rpc(server)]
pub trait TestApi {
    #[method(name = "test")]
    async fn test(&self) -> RpcResult<String>;

    #[method(name = "echo")]
    async fn echo(&self, msg: String) -> RpcResult<String>;
}

pub struct TestApiImpl;

#[async_trait::async_trait]
impl TestApiServer for TestApiImpl {
    async fn test(&self) -> RpcResult<String> {
        println!("📞 RPC call received: test()");
        Ok("Hello from test RPC!".to_string())
    }

    async fn echo(&self, msg: String) -> RpcResult<String> {
        println!("📞 RPC call received: echo({})", msg);
        Ok(format!("Echo: {}", msg))
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("🚀 Starting RPC test server...");

    let server = ServerBuilder::default().build("127.0.0.1:9933").await?;

    let api = TestApiImpl;
    let methods = api.into_rpc();

    println!("📡 Built server, starting on 127.0.0.1:9933");
    let handle = server.start(methods);

    println!("⏳ Waiting for server to start...");
    tokio::time::sleep(tokio::time::Duration::from_millis(2000)).await;

    // Test if port is accessible
    match tokio::net::TcpStream::connect("127.0.0.1:9933").await {
        Ok(_) => println!("✅ Port 9933 is accessible!"),
        Err(e) => println!("❌ Port 9933 not accessible: {}", e),
    }

    println!("✅ Server started!");
    println!("\n🧪 Test commands:");
    println!("curl -X POST http://127.0.0.1:9933 \\");
    println!("  -H 'Content-Type: application/json' \\");
    println!("  -d '{{\"jsonrpc\":\"2.0\",\"method\":\"test\",\"params\":[],\"id\":1}}'");
    println!();
    println!("curl -X POST http://127.0.0.1:9933 \\");
    println!("  -H 'Content-Type: application/json' \\");
    println!(
        "  -d '{{\"jsonrpc\":\"2.0\",\"method\":\"echo\",\"params\":[\"Hello World\"],\"id\":2}}'"
    );
    println!("\n📊 Check port status:");
    println!("ss -tlnp | grep 9933");
    println!("\n🛑 Press Ctrl+C to stop");

    // Keep running until Ctrl+C
    tokio::signal::ctrl_c().await?;

    println!("\n🛑 Stopping server...");
    handle.stop()?;
    println!("✅ Server stopped");

    Ok(())
}

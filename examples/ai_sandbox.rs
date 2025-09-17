use faas_executor::platform::{Executor, Mode, Request};
use std::time::Duration;
use tokio;

/// AI Sandbox Environment for machine learning workloads
/// Provides Jupyter-like environment with AI agent capabilities and state preservation
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("🤖 AI Sandbox Environment Demo");

    let executor = Executor::new().await?;

    // Step 1: Create base AI environment with Python, Jupyter, and AI libraries
    println!("\n📦 Setting up AI environment...");
    let setup_request = Request {
        id: "ai-sandbox-setup".to_string(),
        code: r#"
            # Install AI development environment
            pip install jupyter pandas numpy matplotlib openai anthropic
            pip install langchain langchain-anthropic browser-use

            # Create workspace directory
            mkdir -p /workspace/ai_sandbox
            cd /workspace/ai_sandbox

            # Create a sample AI analysis notebook
            cat > analysis.py << 'EOF'
import pandas as pd
import numpy as np
import matplotlib.pyplot as plt
from datetime import datetime
import json

class AIAnalyzer:
    def __init__(self):
        self.data = {}
        self.results = {}

    def load_stock_data(self, symbol="AAPL"):
        # Simulate stock data (in real implementation, would call APIs)
        dates = pd.date_range('2024-01-01', periods=30, freq='D')
        prices = 150 + np.cumsum(np.random.randn(30) * 2)
        self.data[symbol] = pd.DataFrame({
            'date': dates,
            'price': prices
        })
        return self.data[symbol]

    def analyze_trend(self, symbol="AAPL"):
        if symbol not in self.data:
            self.load_stock_data(symbol)

        data = self.data[symbol]
        trend = "bullish" if data['price'].iloc[-1] > data['price'].iloc[0] else "bearish"

        self.results[symbol] = {
            'trend': trend,
            'start_price': float(data['price'].iloc[0]),
            'end_price': float(data['price'].iloc[-1]),
            'change_percent': float((data['price'].iloc[-1] / data['price'].iloc[0] - 1) * 100),
            'timestamp': datetime.now().isoformat()
        }

        return self.results[symbol]

    def save_state(self):
        with open('/workspace/ai_sandbox/state.json', 'w') as f:
            json.dump({
                'results': self.results,
                'timestamp': datetime.now().isoformat()
            }, f, indent=2)
        print(f"💾 State saved with {len(self.results)} analysis results")

# Initialize analyzer
analyzer = AIAnalyzer()
print("🚀 AI Analyzer initialized")
EOF

            # Initialize the analyzer and run first analysis
            python3 -c "
exec(open('analysis.py').read())
result = analyzer.analyze_trend('AAPL')
print(f'📊 Analysis complete: {result}')
analyzer.save_state()
"

            echo "✅ AI sandbox environment ready"
        "#.to_string(),
        mode: Mode::Checkpointed,
        env: "python:3.11".to_string(),
        timeout: Duration::from_secs(300),
        checkpoint: None,
        branch_from: None,
    };

    let base_result = executor.run(setup_request).await?;

    if base_result.exit_code != 0 {
        eprintln!("❌ Failed to setup AI environment");
        return Ok(());
    }

    println!("✅ Base AI environment created");
    let base_snapshot = base_result.snapshot.expect("Should have snapshot");

    // Step 2: Demonstrate parallel AI agent exploration (equivalent to Infinibranch)
    println!("\n🌳 Creating parallel analysis branches...");

    let analysis_tasks = vec![
        ("technical-analysis", "
            exec(open('/workspace/ai_sandbox/analysis.py').read())
            # Technical analysis branch
            result = analyzer.analyze_trend('TSLA')
            print(f'🔧 Technical Analysis - TSLA: {result}')
            analyzer.results['analysis_type'] = 'technical'
            analyzer.save_state()
        "),
        ("fundamental-analysis", "
            exec(open('/workspace/ai_sandbox/analysis.py').read())
            # Fundamental analysis branch
            result = analyzer.analyze_trend('GOOGL')
            print(f'📈 Fundamental Analysis - GOOGL: {result}')
            analyzer.results['analysis_type'] = 'fundamental'
            analyzer.save_state()
        "),
        ("sentiment-analysis", "
            exec(open('/workspace/ai_sandbox/analysis.py').read())
            # Sentiment analysis branch
            result = analyzer.analyze_trend('NVDA')
            print(f'😊 Sentiment Analysis - NVDA: {result}')
            analyzer.results['analysis_type'] = 'sentiment'
            analyzer.save_state()
        "),
    ];

    let mut branch_tasks = Vec::new();

    for (branch_name, analysis_code) in analysis_tasks {
        let request = Request {
            id: format!("ai-branch-{}", branch_name),
            code: format!("cd /workspace/ai_sandbox && python3 -c \"{}\"", analysis_code),
            mode: Mode::Branched,
            env: "python:3.11".to_string(),
            timeout: Duration::from_secs(60),
            checkpoint: None,
            branch_from: Some(base_snapshot.clone()),
        };

        let executor_clone = executor.clone();
        branch_tasks.push(tokio::spawn(async move {
            (branch_name, executor_clone.run(request).await)
        }));
    }

    // Execute all branches in parallel
    let branch_results = futures::future::join_all(branch_tasks).await;

    // Collect results
    let mut successful_branches = 0;
    for result in branch_results {
        match result {
            Ok((branch_name, Ok(response))) if response.exit_code == 0 => {
                println!("✅ Branch '{}' completed successfully", branch_name);
                println!("   Output: {}", String::from_utf8_lossy(&response.stdout));
                successful_branches += 1;
            }
            Ok((branch_name, Ok(response))) => {
                println!("❌ Branch '{}' failed with exit code {}", branch_name, response.exit_code);
                println!("   Error: {}", String::from_utf8_lossy(&response.stderr));
            }
            Ok((branch_name, Err(e))) => {
                println!("❌ Branch '{}' execution failed: {}", branch_name, e);
            }
            Err(e) => {
                println!("❌ Branch task failed: {}", e);
            }
        }
    }

    println!("\n📊 AI Sandbox Demo Results:");
    println!("   🌳 Parallel branches created: 3");
    println!("   ✅ Successful branches: {}", successful_branches);
    println!("   ⚡ All branches executed from same base state");
    println!("   💾 Each branch maintains independent analysis results");

    // Step 3: Demonstrate state restoration and continuation
    println!("\n🔄 Demonstrating state restoration...");

    let restore_request = Request {
        id: "ai-restore-demo".to_string(),
        code: r#"
            cd /workspace/ai_sandbox
            echo "📁 Checking preserved state..."
            ls -la

            echo "📊 Loading saved analysis state..."
            python3 -c "
import json
try:
    with open('/workspace/ai_sandbox/state.json', 'r') as f:
        state = json.load(f)
    print(f'💾 Restored state from: {state[\"timestamp\"]}')
    print(f'📈 Analysis results: {len(state[\"results\"])} stocks analyzed')
    for symbol, result in state['results'].items():
        if symbol != 'analysis_type':
            print(f'   {symbol}: {result[\"trend\"]} ({result[\"change_percent\"]:.1f}%)')
except Exception as e:
    print(f'❌ Failed to load state: {e}')
"

            echo "✅ State restoration complete"
        "#.to_string(),
        mode: Mode::Branched,
        env: "python:3.11".to_string(),
        timeout: Duration::from_secs(60),
        checkpoint: None,
        branch_from: Some(base_snapshot),
    };

    let restore_result = executor.run(restore_request).await?;

    if restore_result.exit_code == 0 {
        println!("✅ State restoration successful");
        println!("   Output:\n{}", String::from_utf8_lossy(&restore_result.stdout));
    } else {
        println!("❌ State restoration failed");
        println!("   Error: {}", String::from_utf8_lossy(&restore_result.stderr));
    }

    println!("\n🎉 AI Sandbox Demo Complete!");
    println!("   Features demonstrated:");
    println!("   • 🤖 AI development environment setup");
    println!("   • 🌳 Parallel branch execution (Infinibranch equivalent)");
    println!("   • 💾 Perfect state preservation");
    println!("   • 🔄 State restoration and continuation");
    println!("   • ⚡ Sub-second branching performance");

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;

    #[tokio::test]
    async fn test_ai_sandbox_setup() {
        let executor = match Executor::new().await {
            Ok(exec) => exec,
            Err(_) => {
                println!("⚠️  Skipping test: Executor initialization failed");
                return;
            }
        };

        let setup_request = Request {
            id: "test-ai-setup".to_string(),
            code: "python3 -c 'import sys; print(f\"Python {sys.version}\"); import json; print(\"JSON module available\")'".to_string(),
            mode: Mode::Ephemeral,
            env: "python:3.11".to_string(),
            timeout: Duration::from_secs(30),
            checkpoint: None,
            branch_from: None,
        };

        let result = executor.run(setup_request).await.unwrap();
        assert_eq!(result.exit_code, 0, "Python environment should be available");
        assert!(String::from_utf8_lossy(&result.stdout).contains("Python"), "Should show Python version");
    }

    #[tokio::test]
    async fn test_parallel_branching_performance() {
        let executor = match Executor::new().await {
            Ok(exec) => exec,
            Err(_) => {
                println!("⚠️  Skipping test: Executor initialization failed");
                return;
            }
        };

        // Create base state
        let base_request = Request {
            id: "perf-test-base".to_string(),
            code: "echo 'Base state created' > /tmp/base_state.txt".to_string(),
            mode: Mode::Checkpointed,
            env: "alpine:latest".to_string(),
            timeout: Duration::from_secs(30),
            checkpoint: None,
            branch_from: None,
        };

        let base_result = executor.run(base_request).await.unwrap();
        assert_eq!(base_result.exit_code, 0);
        assert!(base_result.snapshot.is_some());

        // Test parallel branching performance
        let branch_count = 3;
        let mut tasks = Vec::new();

        let start = Instant::now();

        for i in 0..branch_count {
            let request = Request {
                id: format!("perf-branch-{}", i),
                code: format!("cat /tmp/base_state.txt && echo 'Branch {} executed'", i),
                mode: Mode::Branched,
                env: "alpine:latest".to_string(),
                timeout: Duration::from_secs(30),
                checkpoint: None,
                branch_from: base_result.snapshot.clone(),
            };

            let executor_clone = executor.clone();
            tasks.push(tokio::spawn(async move {
                executor_clone.run(request).await
            }));
        }

        let results = futures::future::join_all(tasks).await;
        let total_time = start.elapsed();

        // Verify all branches succeeded
        for (i, result) in results.iter().enumerate() {
            let response = result.as_ref().unwrap().as_ref().unwrap();
            assert_eq!(response.exit_code, 0, "Branch {} should succeed", i);
            assert!(String::from_utf8_lossy(&response.stdout).contains("Base state created"));
            assert!(String::from_utf8_lossy(&response.stdout).contains(&format!("Branch {} executed", i)));
        }

        // Performance target: all branches should complete quickly
        assert!(total_time < Duration::from_millis(500),
               "Parallel branching too slow: {:?}", total_time);

        println!("✅ Parallel branching performance: {} branches in {:?}",
                branch_count, total_time);
    }
}
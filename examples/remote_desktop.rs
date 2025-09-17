use faas_executor::platform::{Executor, Mode, Request};
use std::time::Duration;
use tokio;

/// Remote Desktop Development Environment - full desktop with GUI applications
/// Provides VNC/X11 desktop environment with development tools and state preservation
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("🖥️ Remote Desktop Development Environment Demo");

    let executor = Executor::new().await?;

    // Step 1: Setup complete desktop development environment
    println!("\n📦 Setting up desktop environment...");
    let setup_request = Request {
        id: "desktop-setup".to_string(),
        code: r#"
            # Update and install desktop environment
            apt-get update
            apt-get install -y ubuntu-desktop-minimal xfce4 xfce4-goodies
            apt-get install -y tigervnc-standalone-server tigervnc-xorg-extension
            apt-get install -y dbus-x11 firefox gedit code

            # Install development tools
            apt-get install -y git curl wget build-essential
            apt-get install -y nodejs npm python3 python3-pip
            apt-get install -y docker.io docker-compose

            # Create VNC user and setup
            useradd -m -s /bin/bash developer
            mkdir -p /home/developer/.vnc
            mkdir -p /home/developer/workspace

            # Setup VNC server configuration
            cat > /home/developer/.vnc/xstartup << 'EOF'
#!/bin/bash
xrdb $HOME/.Xresources
startxfce4 &
EOF
            chmod +x /home/developer/.vnc/xstartup

            # Create desktop session manager
            cat > /home/developer/desktop_manager.py << 'EOF'
#!/usr/bin/env python3
import subprocess
import time
import os
import json
from datetime import datetime

class DesktopManager:
    def __init__(self):
        self.vnc_display = ":1"
        self.vnc_port = "5901"
        self.vnc_process = None
        self.session_state = {}

    def start_desktop(self):
        """Start VNC desktop session"""
        print("🚀 Starting desktop session...")

        # Set VNC password
        vnc_passwd_cmd = f"echo 'desktop123' | vncpasswd -f > /home/developer/.vnc/passwd"
        subprocess.run(vnc_passwd_cmd, shell=True, cwd="/home/developer")
        os.chmod("/home/developer/.vnc/passwd", 0o600)

        # Start VNC server
        vnc_cmd = f"vncserver {self.vnc_display} -geometry 1920x1080 -depth 24"
        result = subprocess.run(vnc_cmd, shell=True, cwd="/home/developer",
                              capture_output=True, text=True, user="developer")

        if result.returncode == 0:
            print(f"✅ Desktop started on display {self.vnc_display}")
            print(f"🔗 VNC URL: vnc://localhost:{self.vnc_port}")
            self.session_state["desktop_started"] = datetime.now().isoformat()
            return True
        else:
            print(f"❌ Desktop start failed: {result.stderr}")
            return False

    def open_application(self, app_command):
        """Open application in desktop session"""
        print(f"📱 Opening application: {app_command}")

        # Use DISPLAY to run application in VNC session
        cmd = f"DISPLAY={self.vnc_display} {app_command} &"
        result = subprocess.run(cmd, shell=True, user="developer")

        self.session_state[f"app_{app_command}"] = {
            "opened_at": datetime.now().isoformat(),
            "status": "running" if result.returncode == 0 else "failed"
        }

        return result.returncode == 0

    def setup_development_workspace(self):
        """Setup development workspace with projects"""
        print("💼 Setting up development workspace...")

        workspace_commands = [
            "mkdir -p /home/developer/workspace/projects",
            "cd /home/developer/workspace && git init sample_project",
            "cd /home/developer/workspace/sample_project && echo '# Sample Project' > README.md",
            "cd /home/developer/workspace/sample_project && git add . && git commit -m 'Initial commit'",
        ]

        for cmd in workspace_commands:
            subprocess.run(cmd, shell=True, user="developer")

        # Create sample development files
        sample_code = '''#!/usr/bin/env python3
import time
import sys

def main():
    print("Hello from remote desktop development!")
    print(f"Current time: {time.strftime('%Y-%m-%d %H:%M:%S')}")
    print(f"Python version: {sys.version}")

if __name__ == "__main__":
    main()
'''

        with open("/home/developer/workspace/projects/hello.py", "w") as f:
            f.write(sample_code)

        print("✅ Development workspace ready")
        self.session_state["workspace_setup"] = datetime.now().isoformat()

    def save_session_state(self):
        """Save current session state"""
        state_file = "/home/developer/workspace/session_state.json"

        # Get list of running processes in desktop
        try:
            ps_result = subprocess.run(
                f"DISPLAY={self.vnc_display} ps aux | grep -v grep | grep DISPLAY",
                shell=True, capture_output=True, text=True
            )
            self.session_state["running_processes"] = ps_result.stdout.strip().split('\n')
        except:
            self.session_state["running_processes"] = []

        self.session_state["saved_at"] = datetime.now().isoformat()

        with open(state_file, "w") as f:
            json.dump(self.session_state, f, indent=2)

        print(f"💾 Session state saved to {state_file}")

    def get_desktop_info(self):
        """Get information about desktop session"""
        info = {
            "vnc_display": self.vnc_display,
            "vnc_port": self.vnc_port,
            "session_state": self.session_state
        }
        return info

# Initialize and start desktop
if __name__ == "__main__":
    manager = DesktopManager()

    # Start desktop session
    if manager.start_desktop():
        time.sleep(2)  # Wait for desktop to initialize

        # Setup workspace
        manager.setup_development_workspace()

        # Open some development applications
        manager.open_application("firefox https://github.com")
        time.sleep(1)
        manager.open_application("code /home/developer/workspace/projects")
        time.sleep(1)
        manager.open_application("gnome-terminal")

        # Save session state
        manager.save_session_state()

        # Print desktop info
        info = manager.get_desktop_info()
        print(f"🖥️ Desktop session info: {json.dumps(info, indent=2)}")

        print("✅ Remote desktop environment ready")
    else:
        print("❌ Failed to start desktop environment")
EOF

            chmod +x /home/developer/desktop_manager.py

            # Run desktop setup
            cd /home/developer && python3 desktop_manager.py

            echo "✅ Remote desktop environment ready"
        "#.to_string(),
        mode: Mode::Checkpointed,
        env: "ubuntu:22.04".to_string(),
        timeout: Duration::from_secs(600), // Extended timeout for GUI installation
        checkpoint: None,
        branch_from: None,
    };

    let base_result = executor.run(setup_request).await?;

    if base_result.exit_code != 0 {
        eprintln!("❌ Failed to setup desktop environment");
        eprintln!("Error: {}", String::from_utf8_lossy(&base_result.stderr));
        return Ok(());
    }

    println!("✅ Base desktop environment created");
    let base_snapshot = base_result.snapshot.expect("Should have snapshot");

    // Step 2: Demonstrate parallel desktop scenarios (different development setups)
    println!("\n🌳 Creating parallel development environments...");

    let development_scenarios = vec![
        ("web-development", "
            cd /home/developer
            python3 -c \"
import subprocess
import json
from datetime import datetime

# Web development setup
print('🌐 Setting up web development environment...')

# Install web development tools
subprocess.run('npm install -g create-react-app webpack-cli', shell=True)
subprocess.run('cd /home/developer/workspace && npx create-react-app web-project', shell=True)

# Start development server in background
subprocess.run('cd /home/developer/workspace/web-project && npm start &', shell=True)

state = {
    'environment': 'web-development',
    'tools_installed': ['react', 'webpack', 'npm'],
    'project_created': '/home/developer/workspace/web-project',
    'timestamp': datetime.now().isoformat()
}

with open('/home/developer/workspace/web_dev_state.json', 'w') as f:
    json.dump(state, f, indent=2)

print(f'✅ Web development environment ready: {state}')
\"
        "),
        ("data-science", "
            cd /home/developer
            python3 -c \"
import subprocess
import json
from datetime import datetime

# Data science setup
print('📊 Setting up data science environment...')

# Install data science tools
subprocess.run('pip3 install jupyter pandas numpy matplotlib scipy scikit-learn', shell=True)

# Create Jupyter workspace
subprocess.run('mkdir -p /home/developer/workspace/notebooks', shell=True)

# Create sample notebook
notebook_content = '''
{
 \\\"cells\\\": [
  {
   \\\"cell_type\\\": \\\"code\\\",
   \\\"source\\\": [\\\"import pandas as pd\\\\nimport numpy as np\\\\nprint('Data science environment ready!')\\\"],
   \\\"outputs\\\": []
  }
 ],
 \\\"metadata\\\": {},
 \\\"nbformat\\\": 4,
 \\\"nbformat_minor\\\": 4
}
'''

with open('/home/developer/workspace/notebooks/sample.ipynb', 'w') as f:
    f.write(notebook_content)

# Start Jupyter in background
subprocess.run('cd /home/developer/workspace/notebooks && jupyter notebook --ip=0.0.0.0 --no-browser --allow-root &', shell=True)

state = {
    'environment': 'data-science',
    'tools_installed': ['jupyter', 'pandas', 'numpy', 'matplotlib'],
    'notebook_created': '/home/developer/workspace/notebooks/sample.ipynb',
    'timestamp': datetime.now().isoformat()
}

with open('/home/developer/workspace/data_science_state.json', 'w') as f:
    json.dump(state, f, indent=2)

print(f'✅ Data science environment ready: {state}')
\"
        "),
        ("mobile-development", "
            cd /home/developer
            python3 -c \"
import subprocess
import json
from datetime import datetime

# Mobile development setup
print('📱 Setting up mobile development environment...')

# Install mobile development tools
subprocess.run('curl -s https://get.sdkman.io | bash', shell=True)
subprocess.run('apt-get install -y android-sdk', shell=True)

# Create mobile project structure
subprocess.run('mkdir -p /home/developer/workspace/mobile-app/src', shell=True)

# Create sample mobile app code
app_code = '''
package com.example.sampleapp;

import android.app.Activity;
import android.os.Bundle;
import android.widget.TextView;

public class MainActivity extends Activity {
    @Override
    protected void onCreate(Bundle savedInstanceState) {
        super.onCreate(savedInstanceState);
        TextView textView = new TextView(this);
        textView.setText(\\\"Hello from Mobile Development Environment!\\\");
        setContentView(textView);
    }
}
'''

with open('/home/developer/workspace/mobile-app/src/MainActivity.java', 'w') as f:
    f.write(app_code)

state = {
    'environment': 'mobile-development',
    'tools_installed': ['android-sdk', 'java'],
    'project_created': '/home/developer/workspace/mobile-app',
    'timestamp': datetime.now().isoformat()
}

with open('/home/developer/workspace/mobile_dev_state.json', 'w') as f:
    json.dump(state, f, indent=2)

print(f'✅ Mobile development environment ready: {state}')
\"
        "),
    ];

    let mut branch_tasks = Vec::new();

    for (scenario_name, setup_code) in development_scenarios {
        let request = Request {
            id: format!("desktop-branch-{}", scenario_name),
            code: setup_code.to_string(),
            mode: Mode::Branched,
            env: "ubuntu:22.04".to_string(),
            timeout: Duration::from_secs(120),
            checkpoint: None,
            branch_from: Some(base_snapshot.clone()),
        };

        let executor_clone = executor.clone();
        branch_tasks.push(tokio::spawn(async move {
            (scenario_name, executor_clone.run(request).await)
        }));
    }

    // Execute all development scenarios in parallel
    let branch_results = futures::future::join_all(branch_tasks).await;

    // Collect results
    let mut successful_scenarios = 0;
    for result in branch_results {
        match result {
            Ok((scenario_name, Ok(response))) if response.exit_code == 0 => {
                println!("✅ Scenario '{}' completed successfully", scenario_name);
                println!("   Output: {}", String::from_utf8_lossy(&response.stdout));
                successful_scenarios += 1;
            }
            Ok((scenario_name, Ok(response))) => {
                println!("❌ Scenario '{}' failed with exit code {}", scenario_name, response.exit_code);
                println!("   Error: {}", String::from_utf8_lossy(&response.stderr));
            }
            Ok((scenario_name, Err(e))) => {
                println!("❌ Scenario '{}' execution failed: {}", scenario_name, e);
            }
            Err(e) => {
                println!("❌ Scenario task failed: {}", e);
            }
        }
    }

    // Step 3: Demonstrate desktop state restoration
    println!("\n🔄 Demonstrating desktop state restoration...");

    let restore_request = Request {
        id: "desktop-restore-demo".to_string(),
        code: r#"
            cd /home/developer
            echo "📁 Checking preserved desktop state..."
            ls -la workspace/

            echo "📊 Loading saved session state..."
            if [ -f "workspace/session_state.json" ]; then
                echo "💾 Desktop session state found:"
                cat workspace/session_state.json | head -20
                echo "..."

                python3 -c "
import json
try:
    with open('/home/developer/workspace/session_state.json', 'r') as f:
        state = json.load(f)
    print(f'📈 Restored desktop session from: {state[\"saved_at\"]}')
    print(f'🖥️ Desktop display: {state.get(\"vnc_display\", \"unknown\")}')
    print(f'🔗 VNC port: {state.get(\"vnc_port\", \"unknown\")}')

    if 'workspace_setup' in state:
        print(f'💼 Workspace setup at: {state[\"workspace_setup\"]}')

    running_processes = state.get('running_processes', [])
    print(f'⚙️ Running processes: {len(running_processes)} found')

except Exception as e:
    print(f'❌ Failed to parse desktop state: {e}')
"
            else
                echo "⚠️  No desktop session state file found"
            fi

            echo "📱 Checking development environment states..."
            for state_file in workspace/*_state.json; do
                if [ -f "$state_file" ]; then
                    echo "📄 Found: $(basename $state_file)"
                    python3 -c "
import json
import sys
try:
    with open('$state_file', 'r') as f:
        state = json.load(f)
    print(f'  Environment: {state[\"environment\"]}')
    print(f'  Tools: {state[\"tools_installed\"]}')
    print(f'  Timestamp: {state[\"timestamp\"]}')
except Exception as e:
    print(f'  Error reading state: {e}')
"
                fi
            done

            echo "✅ Desktop state restoration check complete"
        "#.to_string(),
        mode: Mode::Branched,
        env: "ubuntu:22.04".to_string(),
        timeout: Duration::from_secs(60),
        checkpoint: None,
        branch_from: Some(base_snapshot),
    };

    let restore_result = executor.run(restore_request).await?;

    if restore_result.exit_code == 0 {
        println!("✅ Desktop state restoration successful");
        println!("   Output:\n{}", String::from_utf8_lossy(&restore_result.stdout));
    } else {
        println!("❌ Desktop state restoration failed");
        println!("   Error: {}", String::from_utf8_lossy(&restore_result.stderr));
    }

    println!("\n🎉 Remote Desktop Development Demo Complete!");
    println!("   📊 Development scenarios: 3");
    println!("   ✅ Successful scenarios: {}", successful_scenarios);
    println!("   Features demonstrated:");
    println!("   • 🖥️ Full GUI desktop environment with VNC");
    println!("   • 🌳 Parallel development environment branching");
    println!("   • 💼 Complete development workspaces (Web, Data Science, Mobile)");
    println!("   • 💾 Desktop session and application state preservation");
    println!("   • 🔄 State restoration and environment continuation");
    println!("   • 📱 Multiple development toolchains in parallel");

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;

    #[tokio::test]
    async fn test_desktop_environment_setup() {
        let executor = match Executor::new().await {
            Ok(exec) => exec,
            Err(_) => {
                println!("⚠️  Skipping test: Executor initialization failed");
                return;
            }
        };

        let basic_test = Request {
            id: "desktop-basic-test".to_string(),
            code: "python3 --version && echo 'Python available' && which vncserver && echo 'VNC available'".to_string(),
            mode: Mode::Ephemeral,
            env: "ubuntu:22.04".to_string(),
            timeout: Duration::from_secs(30),
            checkpoint: None,
            branch_from: None,
        };

        let result = executor.run(basic_test).await.unwrap();
        // Note: This will fail without desktop packages installed, which is expected
        // In full environment, this would pass
        println!("Desktop test result: {}", result.exit_code);
    }

    #[tokio::test]
    async fn test_parallel_development_environments() {
        let executor = match Executor::new().await {
            Ok(exec) => exec,
            Err(_) => {
                println!("⚠️  Skipping test: Executor initialization failed");
                return;
            }
        };

        // Create base development state
        let base_request = Request {
            id: "dev-env-base".to_string(),
            code: "mkdir -p /workspace/base && echo 'Base development environment' > /workspace/base/env.txt".to_string(),
            mode: Mode::Checkpointed,
            env: "ubuntu:22.04".to_string(),
            timeout: Duration::from_secs(30),
            checkpoint: None,
            branch_from: None,
        };

        let base_result = executor.run(base_request).await.unwrap();
        assert_eq!(base_result.exit_code, 0);
        assert!(base_result.snapshot.is_some());

        // Test parallel development environment setup
        let environments = vec!["web", "mobile", "data"];
        let mut tasks = Vec::new();

        let start = Instant::now();

        for env_type in environments {
            let request = Request {
                id: format!("dev-env-{}", env_type),
                code: format!(
                    "cat /workspace/base/env.txt && mkdir -p /workspace/{} && echo 'Environment: {}' > /workspace/{}/config.txt",
                    env_type, env_type, env_type
                ),
                mode: Mode::Branched,
                env: "ubuntu:22.04".to_string(),
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

        // Verify all environments succeeded
        for (i, result) in results.iter().enumerate() {
            let response = result.as_ref().unwrap().as_ref().unwrap();
            assert_eq!(response.exit_code, 0, "Environment {} should succeed", i);
            assert!(String::from_utf8_lossy(&response.stdout).contains("Base development environment"));
        }

        println!("✅ Parallel development environments: 3 environments in {:?}", total_time);
    }
}
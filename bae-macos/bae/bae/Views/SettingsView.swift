import SwiftUI

struct SettingsView: View {
    let appService: AppService

    var body: some View {
        TabView {
            SubsonicSettingsTab(appService: appService)
                .tabItem {
                    Label("Server", systemImage: "network")
                }

            AboutTab()
                .tabItem {
                    Label("About", systemImage: "info.circle")
                }
        }
        .frame(width: 450, height: 300)
    }
}

// MARK: - Subsonic Server Tab

struct SubsonicSettingsTab: View {
    let appService: AppService

    @State private var isRunning = false
    @State private var isToggling = false
    @State private var error: String?

    var body: some View {
        Form {
            Section {
                HStack {
                    Circle()
                        .fill(isRunning ? Color.green : Color.secondary.opacity(0.4))
                        .frame(width: 8, height: 8)
                    Text(isRunning ? "Running" : "Stopped")
                        .foregroundStyle(isRunning ? .primary : .secondary)
                }

                LabeledContent("Port") {
                    Text("\(appService.appHandle.serverPort())")
                        .monospaced()
                }
                LabeledContent("Bind Address") {
                    Text(appService.appHandle.serverBindAddress())
                        .monospaced()
                }

                Toggle("Enable Subsonic Server", isOn: Binding(
                    get: { isRunning },
                    set: { toggleServer($0) }
                ))
                .disabled(isToggling)

                if let error {
                    Text(error)
                        .foregroundStyle(.red)
                        .font(.callout)
                }
            } header: {
                Text("Subsonic API Server")
            } footer: {
                Text("Allows third-party apps to stream music from this library via the Subsonic API.")
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }
        }
        .formStyle(.grouped)
        .padding()
        .onAppear {
            isRunning = appService.appHandle.isSubsonicRunning()
        }
    }

    private func toggleServer(_ enable: Bool) {
        isToggling = true
        error = nil
        let handle = appService.appHandle

        Task.detached {
            do {
                if enable {
                    try handle.startSubsonicServer()
                } else {
                    handle.stopSubsonicServer()
                }

                let running = handle.isSubsonicRunning()
                await MainActor.run {
                    isRunning = running
                    isToggling = false
                }
            } catch {
                await MainActor.run {
                    self.error = error.localizedDescription
                    isToggling = false
                }
            }
        }
    }
}

// MARK: - About Tab

struct AboutTab: View {
    var body: some View {
        VStack(spacing: 16) {
            Spacer()

            Text("bae")
                .font(.system(size: 36, weight: .bold, design: .rounded))

            if let version = Bundle.main.infoDictionary?["CFBundleShortVersionString"] as? String {
                Text("Version \(version)")
                    .font(.callout)
                    .foregroundStyle(.secondary)
            }

            if let build = Bundle.main.infoDictionary?["CFBundleVersion"] as? String {
                Text("Build \(build)")
                    .font(.caption)
                    .foregroundStyle(.tertiary)
            }

            Spacer()
        }
        .frame(maxWidth: .infinity)
    }
}

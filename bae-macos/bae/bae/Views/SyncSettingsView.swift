import SwiftUI

struct SyncSettingsView: View {
    let appService: AppService

    @State private var syncStatus: BridgeSyncStatus?
    @State private var syncConfig: BridgeSyncConfig?
    @State private var members: [BridgeMember] = []
    @State private var userPubkey: String?
    @State private var followCode: String?
    @State private var error: String?
    @State private var copiedField: String?

    // Editable S3 config fields
    @State private var bucket = ""
    @State private var region = ""
    @State private var endpoint = ""
    @State private var keyPrefix = ""
    @State private var accessKey = ""
    @State private var secretKey = ""
    @State private var shareBaseUrl = ""

    var body: some View {
        Form {
            syncStatusSection
            s3ConfigSection
            identitySection
            membershipSection
        }
        .formStyle(.grouped)
        .navigationTitle("Sync")
        .task {
            loadSyncInfo()
        }
    }

    // MARK: - Sync Status

    private var syncStatusSection: some View {
        Section("Status") {
            if let status = syncStatus {
                LabeledContent("Configured") {
                    Text(status.configured ? "Yes" : "No")
                        .foregroundStyle(status.configured ? .green : .secondary)
                }
                if let lastSync = status.lastSyncTime {
                    LabeledContent("Last Sync") {
                        Text(lastSync)
                    }
                }
                if let syncError = status.error {
                    LabeledContent("Error") {
                        Text(syncError)
                            .foregroundStyle(.red)
                    }
                }
                LabeledContent("Devices") {
                    Text("\(status.deviceCount)")
                }
            } else {
                Text("Loading...")
                    .foregroundStyle(.secondary)
            }

            if let error {
                Text(error)
                    .foregroundStyle(.red)
                    .font(.caption)
            }
        }
    }

    // MARK: - S3 Configuration

    private var s3ConfigSection: some View {
        Section("S3 Bucket Configuration") {
            TextField("Bucket", text: $bucket)
            TextField("Region", text: $region)
            TextField("Endpoint (optional)", text: $endpoint)
            TextField("Key Prefix (optional)", text: $keyPrefix)
            SecureField("Access Key", text: $accessKey)
            SecureField("Secret Key", text: $secretKey)
            TextField("Share Base URL (optional)", text: $shareBaseUrl)

            HStack {
                Spacer()
                Button("Save") {
                    saveSyncConfig()
                }
                .disabled(bucket.isEmpty || region.isEmpty || accessKey.isEmpty || secretKey.isEmpty)
            }
        }
    }

    // MARK: - Identity

    private var identitySection: some View {
        Section("Identity") {
            if let pubkey = userPubkey {
                LabeledContent("Public Key") {
                    HStack {
                        Text(truncateKey(pubkey))
                            .font(.system(.body, design: .monospaced))
                            .textSelection(.enabled)
                        copyButton(value: pubkey, field: "pubkey")
                    }
                }
            } else {
                LabeledContent("Public Key") {
                    Text("Not generated")
                        .foregroundStyle(.secondary)
                }
            }

            if syncStatus?.configured == true {
                HStack {
                    if let code = followCode {
                        Text(truncateKey(code))
                            .font(.system(.caption, design: .monospaced))
                            .lineLimit(1)
                            .truncationMode(.middle)
                        copyButton(value: code, field: "followCode")
                    }
                    Spacer()
                    Button("Generate Follow Code") {
                        generateFollowCode()
                    }
                }
            }
        }
    }

    // MARK: - Membership

    private var membershipSection: some View {
        Section("Members") {
            if members.isEmpty {
                Text("No members (solo library)")
                    .foregroundStyle(.secondary)
            } else {
                ForEach(members, id: \.pubkey) { member in
                    HStack {
                        VStack(alignment: .leading) {
                            HStack {
                                Text(member.name ?? truncateKey(member.pubkey))
                                    .font(.headline)
                                Text(member.role)
                                    .font(.caption)
                                    .padding(.horizontal, 6)
                                    .padding(.vertical, 2)
                                    .background(
                                        member.role == "owner"
                                            ? Color.blue.opacity(0.15)
                                            : Color.gray.opacity(0.15)
                                    )
                                    .clipShape(Capsule())
                            }
                            Text(truncateKey(member.pubkey))
                                .font(.system(.caption, design: .monospaced))
                                .foregroundStyle(.secondary)
                        }
                        Spacer()
                        copyButton(value: member.pubkey, field: member.pubkey)
                    }
                }
            }
        }
    }

    // MARK: - Helpers

    private func copyButton(value: String, field: String) -> some View {
        Button {
            NSPasteboard.general.clearContents()
            NSPasteboard.general.setString(value, forType: .string)
            copiedField = field
            DispatchQueue.main.asyncAfter(deadline: .now() + 2) {
                if copiedField == field {
                    copiedField = nil
                }
            }
        } label: {
            Image(systemName: copiedField == field ? "checkmark" : "doc.on.doc")
        }
        .buttonStyle(.borderless)
        .help("Copy to clipboard")
    }

    private func truncateKey(_ key: String) -> String {
        guard key.count > 16 else { return key }
        let start = key.prefix(8)
        let end = key.suffix(8)
        return "\(start)...\(end)"
    }

    private func loadSyncInfo() {
        syncStatus = appService.appHandle.getSyncStatus()
        syncConfig = appService.appHandle.getSyncConfig()
        userPubkey = appService.appHandle.getUserPubkey()

        // Populate editable fields from current config
        if let config = syncConfig {
            bucket = config.s3Bucket ?? ""
            region = config.s3Region ?? ""
            endpoint = config.s3Endpoint ?? ""
            keyPrefix = config.s3KeyPrefix ?? ""
            shareBaseUrl = config.shareBaseUrl ?? ""
        }

        // Load members in background (requires network)
        Task.detached { [appService] in
            do {
                let result = try appService.appHandle.getMembers()
                await MainActor.run {
                    members = result
                }
            } catch {
                await MainActor.run {
                    self.error = "Failed to load members: \(error.localizedDescription)"
                }
            }
        }
    }

    private func saveSyncConfig() {
        let data = BridgeSaveSyncConfig(
            bucket: bucket,
            region: region,
            endpoint: endpoint.isEmpty ? nil : endpoint,
            keyPrefix: keyPrefix.isEmpty ? nil : keyPrefix,
            accessKey: accessKey,
            secretKey: secretKey,
            shareBaseUrl: shareBaseUrl.isEmpty ? nil : shareBaseUrl
        )
        do {
            try appService.appHandle.saveSyncConfig(configData: data)
            error = nil
            // Reload status after saving
            syncStatus = appService.appHandle.getSyncStatus()
            syncConfig = appService.appHandle.getSyncConfig()
        } catch {
            self.error = "Failed to save: \(error.localizedDescription)"
        }
    }

    private func generateFollowCode() {
        do {
            followCode = try appService.appHandle.generateFollowCode()
        } catch {
            self.error = "Failed to generate follow code: \(error.localizedDescription)"
        }
    }
}

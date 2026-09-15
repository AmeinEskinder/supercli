//
//  UnpeelMCPSettingsPanel.swift
//  UnpeelNative
//
//  Extracted from SettingsView.swift — Settings ▸ Sessions use panel.
//

import SwiftUI
import UnpeelShared

/// The Sessions-domain controls inside the unified Unpeel MCP settings group.
/// The compatibility feature gate is still `sessionsMcp`; this panel explains
/// terminal-session access and lists remembered write/App-open approvals.
/// It also owns "Connected agents": which agent CLIs on the selected Host
/// have Unpeel's integration (lifecycle hooks + the unpeel MCP server)
/// installed, with the Host verb to connect one and the manual recipe.
struct UnpeelMCPSettingsPanel: View {
    @ObservedObject var store: UnpeelStore
    @ObservedObject var runtime: RemoteHostRuntime
    @State private var pendingIntegrations: Set<String> = []
    @State private var integrationError: String?
    @State private var showManualSetup = false

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            Form {
                Section {} header: {
                    SettingsPaneHeader(
                        title: "Unpeel MCP",
                        description: "Every capable agent gets the unpeel MCP server: your "
                            + "other sessions, a real browser, artifacts, and Apps. Connect an "
                            + "agent once per Host; Unpeel keeps it current after upgrades."
                    )
                    .padding(.bottom, 4)
                }

                connectedAgentsSection
                accessModelSection
                writeAccessSection
                worktreeAccessSection
                gallerySection
                approvedPairsSection
                approvedAppsSection
            }
            .formStyle(.grouped)
            .scrollContentBackground(.hidden)
        }
    }

    // MARK: Connected agents

    private var connectableAgents: [RemoteAgentSummary] {
        (runtime.snapshot?.workspaceSettings?.availableAgents ?? []).filter { $0.installed }
    }
    private var canConnect: Bool {
        runtime.supportsHostOperation(RemoteHostRuntime.HostOperation.integrationsInstall)
    }

    /// One row per agent CLI present on the Host: connected (hooks + MCP
    /// registered in that CLI's own config), connectable, or detection-only.
    /// Connecting runs the Host's `integrations.install` verb; the row copy
    /// comes from the runtime package, so the Controller knows nothing about
    /// provider file formats.
    private var connectedAgentsSection: some View {
        Section {
            if connectableAgents.isEmpty {
                Text("No agent CLIs were found on this Host's PATH. Install one under "
                    + "Agents & Apps and it will appear here.")
                    .font(.system(size: 12))
                    .foregroundStyle(Theme.mutedForeground)
            }
            ForEach(connectableAgents) { agent in
                connectedAgentRow(agent)
            }
            if let integrationError {
                Text(integrationError)
                    .font(.system(size: 11))
                    .foregroundStyle(.red)
                    .fixedSize(horizontal: false, vertical: true)
            }
            manualSetup
        } header: {
            SettingsSectionHeader(
                title: "Connected agents",
                description: "An unconnected agent still shows busy and idle from its screen. "
                    + "Connecting registers Unpeel's hooks and the unpeel MCP server in the "
                    + "agent's own configuration, for exact status, notifications, precise "
                    + "resume, and Unpeel's tools inside the agent."
            )
        }
    }

    private func connectedAgentRow(_ agent: RemoteAgentSummary) -> some View {
        let connected = agent.integrationInstalled ?? false
        let installable = agent.integrationInstallable ?? false
        return LabeledContent {
            if !installable {
                Text("Detection only")
                    .font(.system(size: 11))
                    .foregroundStyle(Theme.mutedForeground)
            } else if pendingIntegrations.contains(agent.id) {
                ProgressView().controlSize(.small)
            } else if connected {
                HStack(spacing: 8) {
                    Label("Connected", systemImage: "checkmark.circle.fill")
                        .font(.system(size: 11))
                        .foregroundStyle(Theme.mutedForeground)
                    Button("Reinstall") { connect(agent) }
                        .buttonStyle(.plain)
                        .font(.system(size: 11))
                        .foregroundStyle(Theme.mutedForeground)
                        .disabled(!canConnect)
                        .help("Rewrite the hook script and MCP registration for this Host build")
                }
            } else {
                Button("Connect") { connect(agent) }
                    .buttonStyle(.bordered)
                    .controlSize(.small)
                    .disabled(!canConnect)
                    .help(canConnect
                        ? "Register Unpeel's hooks and MCP server with \(agent.name)"
                        : "This Host does not support connecting agents")
            }
        } label: {
            HStack(alignment: .top, spacing: 10) {
                ToolIconView(appID: nil, command: agent.command, size: 18)
                    .frame(width: 22, height: 22)
                VStack(alignment: .leading, spacing: 2) {
                    Text(agent.name)
                        .font(.system(size: 13))
                        .foregroundStyle(Theme.foreground)
                    Text(installable
                        ? (agent.integrationSummary ?? "Registers Unpeel's hooks and MCP server in the agent's own configuration.")
                        : "This agent has no hooks or MCP configuration Unpeel can register; it keeps identity from detection and resume from its own continue-last.")
                        .font(.system(size: 11))
                        .foregroundStyle(Theme.mutedForeground)
                        .fixedSize(horizontal: false, vertical: true)
                }
            }
        }
    }

    private func connect(_ agent: RemoteAgentSummary) {
        pendingIntegrations.insert(agent.id)
        integrationError = nil
        Task { @MainActor in
            defer { pendingIntegrations.remove(agent.id) }
            do { try await runtime.installIntegration(runtimeID: agent.id) }
            catch { integrationError = "\(agent.name): \(error.localizedDescription)" }
        }
    }

    /// The transparent alternative: the shim path and each provider's own
    /// command. Registering by hand gives the MCP half only; hooks still
    /// need Connect.
    @ViewBuilder
    private var manualSetup: some View {
        let shim = runtime.snapshot?.workspaceSettings?.mcpShimPath
        let manual = connectableAgents.compactMap { agent -> (String, String)? in
            guard let command = agent.integrationManualCommand else { return nil }
            return (agent.name, command)
        }
        DisclosureGroup("Manual setup", isExpanded: $showManualSetup) {
            VStack(alignment: .leading, spacing: 6) {
                Text("Every provider's MCP config points at one launcher on this Host. "
                    + "Register it with the agent's own command to get Unpeel's tools "
                    + "without the hooks; \"Connect\" does both.")
                    .font(.system(size: 11))
                    .foregroundStyle(Theme.mutedForeground)
                    .fixedSize(horizontal: false, vertical: true)
                if let shim {
                    Text(shim)
                        .font(.system(size: 11, design: .monospaced))
                        .textSelection(.enabled)
                }
                ForEach(manual, id: \.0) { name, command in
                    VStack(alignment: .leading, spacing: 1) {
                        Text(name).font(.system(size: 11)).foregroundStyle(Theme.mutedForeground)
                        Text(command)
                            .font(.system(size: 11, design: .monospaced))
                            .textSelection(.enabled)
                    }
                }
                Text("Or from any terminal: unpeel integrations install <agent>")
                    .font(.system(size: 11, design: .monospaced))
                    .foregroundStyle(Theme.mutedForeground)
            }
            .padding(.top, 4)
        }
        .font(.system(size: 12))
    }

    /// Explains the access model: reads are open everywhere and every write to
    /// another session goes through the policy below.
    private var accessModelSection: some View {
        Section {
            Text("Every session can read every other session. Writing to another session "
                + "follows the setting below — by default Unpeel asks you the first time and "
                + "remembers your answer per pair. Sidebar groups are organizational only, "
                + "and agents never create or close sessions themselves.")
                .font(.system(size: 13))
                .foregroundStyle(Theme.mutedForeground)
                .fixedSize(horizontal: false, vertical: true)
        } header: {
            SettingsSectionHeader(
                title: "Session access",
                description: "How sessions can see and control each other."
            )
        }
    }

    /// The app-wide inter-session write policy. Applied live — the host
    /// re-reads it on every write.
    /// Let sessions create Unpeel-managed worktrees (create_worktree/
    /// list_worktrees actions). Only rendered while the Worktrees
    /// experimental feature is on; session creation stays user-only either
    /// way — this grants checkout prep, not agent spawning.
    @ViewBuilder
    private var worktreeAccessSection: some View {
        if UnpeelFeatureFlags.isEnabled(.worktrees) {
            Section {
                LabeledContent {
                    Toggle(
                        "",
                        isOn: Binding(
                            get: { store.mcpWorktreeAccess },
                            set: { store.setMcpWorktreeAccess($0) }
                        )
                    )
                    .toggleStyle(.switch)
                    .labelsHidden()
                    .controlSize(.small)
                } label: {
                    VStack(alignment: .leading, spacing: 1) {
                        Text("Let sessions create worktrees")
                            .font(.system(size: 13))
                            .foregroundStyle(Theme.foreground)
                        Text("Agents can prepare isolated git worktrees as child projects "
                            + "in the sidebar. Launching sessions into them is still up to "
                            + "you. Applies immediately.")
                            .font(.system(size: 11))
                            .foregroundStyle(Theme.mutedForeground)
                            .fixedSize(horizontal: false, vertical: true)
                    }
                }
            }
        }
    }

    private var writeAccessSection: some View {
        Section {
            LabeledContent {
                Picker(
                    "",
                    selection: Binding(
                        get: { store.mcpNonChildWriteAccess },
                        set: { store.setMcpNonChildWriteAccess($0) }
                    )
                ) {
                    ForEach(McpNonChildWriteAccess.allCases) { policy in
                        Text(policy.label).tag(policy)
                    }
                }
                .labelsHidden()
                .pickerStyle(.menu)
                .fixedSize()
            } label: {
                VStack(alignment: .leading, spacing: 1) {
                    Text("Writing to other sessions")
                        .font(.system(size: 13))
                        .foregroundStyle(Theme.foreground)
                    Text(store.mcpNonChildWriteAccess.detail)
                        .font(.system(size: 11))
                        .foregroundStyle(Theme.mutedForeground)
                        .fixedSize(horizontal: false, vertical: true)
                }
            }
        } header: {
            SettingsSectionHeader(
                title: "Write access",
                description: "What happens when a session writes to another session."
            )
        }
    }

    /// Browser screenshots have always landed in the gallery. Keep that as
    /// the default while letting users who take many diagnostic captures keep
    /// them private until an agent publishes a selected image with
    /// `add_to_gallery`.
    private var gallerySection: some View {
        Section {
            LabeledContent {
                Toggle(
                    "",
                    isOn: Binding(
                        get: { store.mcpAutoAddBrowserScreenshots },
                        set: { store.setMcpAutoAddBrowserScreenshots($0) }
                    )
                )
                .toggleStyle(.switch)
                .labelsHidden()
                .controlSize(.small)
            } label: {
                VStack(alignment: .leading, spacing: 1) {
                    Text("Add browser screenshots automatically")
                        .font(.system(size: 13))
                        .foregroundStyle(Theme.foreground)
                    Text(store.mcpAutoAddBrowserScreenshots
                        ? "Browser MCP screenshots appear in the current session's gallery."
                        : "Browser captures stay out of the gallery until an agent adds a "
                            + "selected image with Sessions use. Explicit phone screenshot "
                            + "requests are still added.")
                        .font(.system(size: 11))
                        .foregroundStyle(Theme.mutedForeground)
                        .fixedSize(horizontal: false, vertical: true)
                }
            }
        } header: {
            SettingsSectionHeader(
                title: "Gallery",
                description: "Choose whether Browser MCP captures are published as they are taken."
            )
        }
    }

    /// The remembered caller→target pairs the user has approved. Revoking a
    /// pair makes the next write ask again.
    private var approvedPairsSection: some View {
        Section {
            let pairs = approvedPairs
            if pairs.isEmpty {
                Text("No approved sessions. When you allow a write from the approval "
                    + "dialog, the pair appears here.")
                    .font(.system(size: 12))
                    .foregroundStyle(Theme.mutedForeground)
                    .fixedSize(horizontal: false, vertical: true)
            } else {
                ForEach(pairs, id: \.id) { pair in
                    LabeledContent {
                        Button("Revoke") {
                            store.revokeMcpWriteApproval(
                                caller: pair.caller, target: pair.target
                            )
                        }
                        .controlSize(.small)
                    } label: {
                        VStack(alignment: .leading, spacing: 1) {
                            Text("\(store.sessionDisplayName(pair.caller)) → "
                                + "\(store.sessionDisplayName(pair.target))")
                                .font(.system(size: 13))
                                .foregroundStyle(Theme.foreground)
                            Text("Can type into the session without asking.")
                                .font(.system(size: 11))
                                .foregroundStyle(Theme.mutedForeground)
                        }
                    }
                }
            }
        } header: {
            SettingsSectionHeader(
                title: "Approved sessions",
                description: "Write approvals you've granted. Each lives until either "
                    + "session is removed."
            )
        }
    }

    private var approvedPairs: [(id: String, caller: String, target: String)] {
        store.mcpWriteApprovals
            .flatMap { caller, targets in
                targets.map { (id: "\(caller)→\($0)", caller: caller, target: $0) }
            }
            .sorted { $0.id < $1.id }
    }

    private var approvedAppsSection: some View {
        Section {
            let approvals = approvedApps
            if approvals.isEmpty {
                Text("No approved Apps. An agent asks before it starts an App panel "
                    + "for the first time.")
                    .font(.system(size: 12))
                    .foregroundStyle(Theme.mutedForeground)
                    .fixedSize(horizontal: false, vertical: true)
            } else {
                ForEach(approvals, id: \.id) { approval in
                    LabeledContent {
                        Button("Revoke") {
                            store.revokeMcpAppOpenApproval(
                                caller: approval.caller, appID: approval.appID
                            )
                        }
                        .controlSize(.small)
                    } label: {
                        VStack(alignment: .leading, spacing: 1) {
                            Text("\(store.sessionDisplayName(approval.caller)) → "
                                + approval.appID)
                                .font(.system(size: 13))
                                .foregroundStyle(Theme.foreground)
                            Text("Can start this App as a companion panel without asking.")
                                .font(.system(size: 11))
                                .foregroundStyle(Theme.mutedForeground)
                        }
                    }
                }
            }
        } header: {
            SettingsSectionHeader(
                title: "Approved Apps",
                description: "App-panel launch approvals you've granted to agent sessions."
            )
        }
    }

    private var approvedApps: [(id: String, caller: String, appID: String)] {
        store.mcpAppOpenApprovals
            .flatMap { caller, apps in
                apps.map { (id: "\(caller)→\($0)", caller: caller, appID: $0) }
            }
            .sorted { $0.id < $1.id }
    }

}

// MARK: - Notifications panel

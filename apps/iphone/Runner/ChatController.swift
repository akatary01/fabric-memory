import AVFoundation
import FabricCore
import UIKit
import UniformTypeIdentifiers

final class ChatController: UIViewController, UITableViewDataSource, UITableViewDelegate,
  UIDocumentPickerDelegate, UITabBarDelegate
{
  private let table = UITableView(frame: .zero, style: .plain), input = UITextView(),
    send = UIButton(type: .system)
  private let status = UILabel(), syncStatus = UILabel(), tabs = UITabBar(),
    composer = UIStackView()
  private let devices = UIScrollView(), tools = UIScrollView()
  private let beaconURL = UITextField(), code = UITextField(), deviceName = UITextField(),
    beaconState = UILabel()
  private var deviceButtons: [UIButton] = []
  private var syncDisplay: Task<Void, Never>?, session: PhoneSession?, sending: Task<Void, Never>?
  private var turns: [SyncClient.Turn] = [], memories: [Event] = [], busy = false, selected = 0,
    pendingLink: URL?
  private let themes: [(String, [UInt32])] = [
    ("Paper", [0xf0efe9, 0xdfdfd9, 0x1c1c1e, 0x4d4d51]),
    ("Sky", [0xd9ecff, 0xc7def5, 0x17384f, 0x29475d]),
    ("Lemon", [0xfff6cc, 0xebe2b6, 0x4e4210, 0x5d4b1f]),
    ("Mint", [0xddf4e5, 0xc8e1d1, 0x1f4c38, 0x315540]),
    ("Rose", [0xffe1dc, 0xeccbc6, 0x5f3431, 0x703f3c]),
    ("Stone", [0xdedede, 0xc9c9c9, 0x2f2f2f, 0x424242]),
  ]
  private var paper: UIColor { palette(0) }
  private var ink: UIColor { palette(2) }
  private var muted: UIColor { palette(3) }
  private func palette(_ index: Int) -> UIColor {
    let value = themes[UserDefaults.standard.integer(forKey: "paperTheme") % themes.count].1[index]
    return UIColor { trait in
      if trait.userInterfaceStyle == .dark {
        return index < 2
          ? UIColor(white: index == 0 ? 0.17 : 0.26, alpha: 1)
          : UIColor(white: index == 2 ? 0.94 : 0.77, alpha: 1)
      }
      return UIColor(
        red: CGFloat((value >> 16) & 255) / 255, green: CGFloat((value >> 8) & 255) / 255,
        blue: CGFloat(value & 255) / 255, alpha: 1)
    }
  }
  override func viewDidLoad() {
    super.viewDidLoad()
    title = "MEMORY FABRIC"
    navigationItem.leftBarButtonItem = UIBarButtonItem(
      barButtonSystemItem: .add, target: self, action: #selector(addNote))
    navigationItem.rightBarButtonItem = UIBarButtonItem(
      image: UIImage(systemName: "paintpalette"), style: .plain, target: self,
      action: #selector(changePaper))
    for label in [status, syncStatus] {
      label.numberOfLines = 0
      label.font = .preferredFont(forTextStyle: .caption1)
      label.adjustsFontForContentSizeCategory = true
    }
    status.text = "Opening local memory…"
    table.dataSource = self
    table.delegate = self
    table.separatorStyle = .singleLine
    table.keyboardDismissMode = .interactive
    table.register(UITableViewCell.self, forCellReuseIdentifier: "turn")
    table.rowHeight = UITableView.automaticDimension
    table.estimatedRowHeight = 90
    input.font = .preferredFont(forTextStyle: .body)
    input.adjustsFontForContentSizeCategory = true
    input.layer.borderWidth = 0.5
    input.layer.cornerRadius = 0
    input.accessibilityLabel = "Message"
    input.textContainerInset = UIEdgeInsets(top: 9, left: 8, bottom: 9, right: 8)
    send.setTitle("Send ↑", for: .normal)
    send.addTarget(self, action: #selector(submit), for: .touchUpInside)
    send.widthAnchor.constraint(equalToConstant: 70).isActive = true
    send.heightAnchor.constraint(greaterThanOrEqualToConstant: 44).isActive = true
    let row = UIStackView(arrangedSubviews: [input, send])
    row.spacing = 8
    row.alignment = .bottom
    input.heightAnchor.constraint(equalToConstant: 78).isActive = true
    let hint = caption("LFM3 · Local model · Web off")
    composer.axis = .vertical
    composer.spacing = 6
    composer.addArrangedSubview(row)
    composer.addArrangedSubview(hint)
    tabs.delegate = self
    tabs.items = [
      ("Chat", "bubble.left"), ("Memory", "square.stack"), ("Tools", "square.grid.2x2"),
      ("Devices", "qrcode"),
    ].enumerated().map {
      UITabBarItem(title: $0.element.0, image: UIImage(systemName: $0.element.1), tag: $0.offset)
    }
    tabs.selectedItem = tabs.items?.first
    prepareDevices()
    prepareTools()
    devices.isHidden = true
    tools.isHidden = true
    let content = UIStackView(arrangedSubviews: [
      status, syncStatus, table, devices, tools, composer, tabs,
    ])
    content.axis = .vertical
    content.spacing = 10
    content.translatesAutoresizingMaskIntoConstraints = false
    view.addSubview(content)
    NSLayoutConstraint.activate([
      content.topAnchor.constraint(equalTo: view.safeAreaLayoutGuide.topAnchor, constant: 10),
      content.leadingAnchor.constraint(equalTo: view.leadingAnchor, constant: 16),
      content.trailingAnchor.constraint(equalTo: view.trailingAnchor, constant: -16),
      content.bottomAnchor.constraint(equalTo: view.keyboardLayoutGuide.topAnchor, constant: -5),
    ])
    applyPaper()
    setBusy(true)
    NotificationCenter.default.addObserver(
      self, selector: #selector(suspendGeneration),
      name: UIApplication.willResignActiveNotification, object: nil)
    NotificationCenter.default.addObserver(
      self, selector: #selector(resumeSync), name: UIApplication.didBecomeActiveNotification,
      object: nil)
    Task {
      do {
        guard let assets = Bundle.main.url(forResource: "DemoAssets", withExtension: nil) else {
          throw FabricError("App model assets are missing")
        }
        let storage = FileManager.default.urls(for: .documentDirectory, in: .userDomainMask)[0]
          .appendingPathComponent("SharedMemory")
        let loaded = try await Task.detached(priority: .userInitiated) {
          try PhoneSession(assets: assets, storage: storage)
        }.value
        session = loaded
        turns = await loaded.conversation()
        table.reloadData()
        status.text = "LFM3 is ready on this iPhone."
        if UIApplication.shared.applicationState == .active { resumeSync() }
        await refreshBeacon()
      } catch { status.text = "Could not open memory: \(error)" }
      setBusy(false)
      if let link = pendingLink {
        pendingLink = nil
        acceptPairingLink(link)
      }
    }
  }
  private func caption(_ text: String) -> UILabel {
    let l = UILabel()
    l.text = text
    l.numberOfLines = 0
    l.font = .preferredFont(forTextStyle: .caption1)
    l.adjustsFontForContentSizeCategory = true
    l.textColor = muted
    return l
  }
  private func heading(_ text: String) -> UILabel {
    let l = caption(text)
    l.font = UIFont.systemFont(ofSize: 12, weight: .medium)
    return l
  }
  private func button(_ title: String, _ action: Selector) -> UIButton {
    let b = UIButton(type: .system)
    b.setTitle(title, for: .normal)
    b.contentHorizontalAlignment = .leading
    b.heightAnchor.constraint(greaterThanOrEqualToConstant: 44).isActive = true
    b.addTarget(self, action: action, for: .touchUpInside)
    return b
  }
  private func stack(in scroll: UIScrollView, _ children: [UIView]) {
    let s = UIStackView(arrangedSubviews: children)
    s.axis = .vertical
    s.spacing = 12
    s.translatesAutoresizingMaskIntoConstraints = false
    scroll.addSubview(s)
    NSLayoutConstraint.activate([
      s.leadingAnchor.constraint(equalTo: scroll.contentLayoutGuide.leadingAnchor),
      s.trailingAnchor.constraint(equalTo: scroll.contentLayoutGuide.trailingAnchor),
      s.topAnchor.constraint(equalTo: scroll.contentLayoutGuide.topAnchor),
      s.bottomAnchor.constraint(equalTo: scroll.contentLayoutGuide.bottomAnchor),
      s.widthAnchor.constraint(equalTo: scroll.frameLayoutGuide.widthAnchor),
    ])
  }
  private func prepareDevices() {
    for field in [beaconURL, code, deviceName] {
      field.borderStyle = .none
      field.layer.borderWidth = 0.5
      field.font = .preferredFont(forTextStyle: .body)
      field.heightAnchor.constraint(greaterThanOrEqualToConstant: 44).isActive = true
      field.autocorrectionType = .no
      field.autocapitalizationType = .none
    }
    beaconURL.placeholder = "HTTPS beacon URL or pairing link"
    beaconURL.text =
      UserDefaults.standard.string(forKey: "beaconAddress") ?? BeaconInvitation.defaultURL
    beaconURL.accessibilityLabel = "Beacon URL or pairing link"
    beaconURL.keyboardType = .URL
    code.placeholder = "Current invitation code"
    code.isSecureTextEntry = true
    code.accessibilityLabel = "Invitation code"
    deviceName.text = "My iPhone"
    deviceName.accessibilityLabel = "Device name"
    beaconState.font = .preferredFont(forTextStyle: .caption1)
    beaconState.numberOfLines = 0
    let scan = button("Scan beacon QR", #selector(scanQR))
    let join = button("Join beacon", #selector(joinBeacon))
    let sync = button("Sync now", #selector(syncNow))
    let disconnect = button("Disconnect this phone", #selector(disconnectBeacon))
    deviceButtons = [scan, join, sync, disconnect]
    stack(
      in: devices,
      [
        heading("MEMORY BEACON"),
        caption("Use your cloud server now, or a room hub later. Local chat works without either."),
        beaconURL, code, deviceName, scan, join,
        button("Save URL for later", #selector(saveAddress)), beaconState, sync, disconnect,
        heading("OTHER OPTIONS"), button("Import a legacy pairing file", #selector(connectMac)),
        button("Diagnostics", #selector(diagnostics)),
        caption(
          "Joining shares this phone’s memory with trusted devices on the beacon. Background sync catches up when iOS allows or you reopen the app."
        ),
      ])
  }
  private func prepareTools() {
    stack(
      in: tools,
      [
        heading("ON THIS IPHONE"),
        caption(
          "LFM3 generates replies locally. MiniLM retrieves saved context. No Mac is required for chat."
        ), heading("SHARED MEMORY"),
        caption(
          "Connect your Memory beacon in Devices. The beacon stores the event ledger; it does not run your model."
        ), heading("ADD YOUR MAC"),
        caption(
          "For desktop generation and web search, run fabric-memory run on your Mac and join the same beacon."
        ),
        caption("Phone cloud, web, MCP, and system-action adapters are not enabled in this build."),
      ])
  }
  private func applyPaper() {
    view.backgroundColor = paper
    view.tintColor = ink
    table.backgroundColor = paper
    table.separatorColor = muted.withAlphaComponent(0.18)
    input.backgroundColor = paper
    input.textColor = ink
    input.layer.borderColor = muted.withAlphaComponent(0.35).cgColor
    tabs.backgroundColor = paper
    tabs.tintColor = ink
    tabs.unselectedItemTintColor = muted
    status.textColor = muted
    syncStatus.textColor = muted
    beaconState.textColor = muted
    for f in [beaconURL, code, deviceName] {
      f.textColor = ink
      f.backgroundColor = paper
      f.layer.borderColor = muted.withAlphaComponent(0.35).cgColor
    }
    navigationController?.navigationBar.tintColor = ink
    let a = UINavigationBarAppearance()
    a.configureWithOpaqueBackground()
    a.backgroundColor = paper
    a.titleTextAttributes = [
      .font: UIFont.systemFont(ofSize: 12, weight: .medium), .foregroundColor: muted,
    ]
    navigationItem.standardAppearance = a
    navigationItem.scrollEdgeAppearance = a
    table.reloadData()
  }
  @objc private func changePaper() {
    let menu = UIAlertController(title: "Paper color", message: nil, preferredStyle: .actionSheet)
    for (i, theme) in themes.enumerated() {
      menu.addAction(
        UIAlertAction(title: theme.0, style: .default) { _ in
          UserDefaults.standard.set(i, forKey: "paperTheme")
          self.applyPaper()
        })
    }
    menu.addAction(UIAlertAction(title: "Cancel", style: .cancel))
    menu.popoverPresentationController?.barButtonItem = navigationItem.rightBarButtonItem
    present(menu, animated: true)
  }
  private func setBusy(_ value: Bool) {
    busy = value
    send.isEnabled = !value && session != nil
    deviceButtons.forEach { $0.isEnabled = !value && session != nil }
    navigationItem.leftBarButtonItem?.isEnabled = !value && session != nil
  }
  func tabBar(_ tabBar: UITabBar, didSelect item: UITabBarItem) {
    selected = item.tag
    table.isHidden = selected > 1
    composer.isHidden = selected != 0
    devices.isHidden = selected != 3
    tools.isHidden = selected != 2
    if selected == 1 { loadMemories() } else if selected == 3 { Task { await refreshBeacon() } }
    table.reloadData()
  }
  private func loadMemories() {
    Task {
      do {
        memories = try await session?.memories() ?? []
        table.reloadData()
      } catch { status.text = "Could not load memories: \(error)" }
    }
  }
  func tableView(_ tableView: UITableView, numberOfRowsInSection section: Int) -> Int {
    selected == 1 ? memories.count : turns.count
  }
  func tableView(_ tableView: UITableView, cellForRowAt indexPath: IndexPath) -> UITableViewCell {
    let c = tableView.dequeueReusableCell(withIdentifier: "turn", for: indexPath)
    c.selectionStyle = .none
    c.backgroundColor = .clear
    var content = c.defaultContentConfiguration()
    if selected == 1 {
      content.text = ((memories[indexPath.row].access?.scope ?? "public")=="public" ? "PUBLIC":"PRIVATE")+" MEMORY"
      content.secondaryText = memories[indexPath.row].text
    } else {
      let turn = turns[indexPath.row]
      c.backgroundColor = turn.role == "user" ? palette(1) : .clear
      content.text = turn.role == "user" ? "YOU" : "FABRIC · ON THIS IPHONE"
      content.secondaryText = turn.content
    }
    content.textProperties.font = UIFont.systemFont(ofSize: 11, weight: .medium)
    content.textProperties.color = muted
    content.secondaryTextProperties.font = .preferredFont(forTextStyle: .body)
    content.secondaryTextProperties.color = ink
    content.secondaryTextProperties.numberOfLines = 0
    content.directionalLayoutMargins = NSDirectionalEdgeInsets(
      top: 13, leading: 0, bottom: 13, trailing: 0)
    c.contentConfiguration = content
    return c
  }
  func tableView(_ tableView: UITableView, didSelectRowAt indexPath: IndexPath) {
    guard selected == 1, let session, !busy else { return }
    let memory = memories[indexPath.row]
    let alert = UIAlertController(
      title: "Forget this memory?", message: memory.text, preferredStyle: .alert)
    alert.addAction(UIAlertAction(title: "Cancel", style: .cancel))
    alert.addAction(
      UIAlertAction(title: "Forget across synced devices", style: .destructive) { _ in
        Task {
          do {
            try await session.forget(memory: memory.memory)
            self.loadMemories()
          } catch { self.status.text = String(describing: error) }
        }
      })
    present(alert, animated: true)
  }
  @objc private func addNote() {
    guard let session, !busy else { return }
    if selected == 0 {
      input.becomeFirstResponder()
      return
    }
    let alert = UIAlertController(title: "Add a memory", message: nil, preferredStyle: .alert)
    alert.addTextField { $0.placeholder = "Remember…" }
    alert.addAction(UIAlertAction(title: "Cancel", style: .cancel))
    alert.addAction(
      UIAlertAction(title: "Save", style: .default) { _ in
        guard let text = alert.textFields?.first?.text, !text.isEmpty else { return }
        Task {
          do {
            try await session.remember(text)
            self.loadMemories()
            self.status.text = "Saved locally."
          } catch { self.status.text = String(describing: error) }
        }
      })
    present(alert, animated: true)
  }
  private func scrollToEnd() {
    if selected == 0 && !turns.isEmpty {
      table.scrollToRow(
        at: IndexPath(row: turns.count - 1, section: 0), at: .bottom, animated: true)
    }
  }
  @objc private func submit() {
    guard !busy, let session else { return }
    let question = input.text.trimmingCharacters(in: .whitespacesAndNewlines)
    guard !question.isEmpty else { return }
    setBusy(true)
    status.text = "Answering on this iPhone…"
    input.resignFirstResponder()
    turns.append(SyncClient.Turn(role: "user", content: question))
    table.reloadData()
    scrollToEnd()
    sending = Task {
      do {
        let answer = try await session.send(question)
        turns = await session.conversation()
        input.text = ""
        status.text = "Answered by \(answer.model)."
      } catch {
        turns = await session.conversation()
        status.text = "Could not answer: \(error). Your prompt is retained."
      }
      table.reloadData()
      scrollToEnd()
      setBusy(false)
    }
  }
  @objc private func resumeSync() {
    guard let session else { return }
    syncDisplay?.cancel()
    syncDisplay = Task { [weak self] in
      await session.startAutomaticSync()
      while !Task.isCancelled {
        let message = await session.automaticSyncStatus()
        guard !Task.isCancelled else { return }
        self?.syncStatus.text = message
        do { try await Task.sleep(nanoseconds: 2_000_000_000) } catch { return }
      }
    }
  }
  deinit {
    syncDisplay?.cancel()
    sending?.cancel()
  }
  @objc private func suspendGeneration() {
    syncDisplay?.cancel()
    syncDisplay = nil
    sending?.cancel()
    if let session {
      Task {
        await session.stopAutomaticSync()
        await session.releaseGenerator()
      }
    }
  }
  private func refreshBeacon() async {
    guard let session else { return }
    let url = await session.beaconURL()
    beaconState.text =
      url.map { "Paired with \($0)." }
      ?? "Not paired. Save an address now and join when your beacon is deployed."
    if let url { beaconURL.text = url }
  }
  @objc private func saveAddress() {
    do {
      let invitation = try BeaconInvitation.parse(beaconURL.text ?? "")
      UserDefaults.standard.set(invitation.url, forKey: "beaconAddress")
      status.text = "Beacon URL saved. Enter an invitation when the beacon is ready."
    } catch { status.text = String(describing: error) }
  }
  @objc private func joinBeacon() {
    guard !busy, let session else { return }
    do {
      let invitation = try BeaconInvitation.parse(beaconURL.text ?? "", code: code.text ?? "")
      let name = deviceName.text ?? "My iPhone"
      let alert = UIAlertController(
        title: "Join this beacon?",
        message:
          "\(invitation.url)\n\nThis phone’s memories will sync with trusted devices on the beacon.",
        preferredStyle: .alert)
      alert.addAction(UIAlertAction(title: "Cancel", style: .cancel))
      alert.addAction(
        UIAlertAction(title: "Join", style: .default) { _ in
          self.setBusy(true)
          Task {
            do {
              let result = try await session.joinBeacon(invitation, name: name)
              self.code.text = ""
              UserDefaults.standard.set(invitation.url, forKey: "beaconAddress")
              self.status.text = "Beacon paired. \(result.downloaded) events downloaded."
              self.resumeSync()
              await self.refreshBeacon()
            } catch { self.status.text = "Pairing failed: \(error)" }
            self.setBusy(false)
          }
        })
      present(alert, animated: true)
    } catch { status.text = String(describing: error) }
  }
  @objc private func scanQR() {
    guard !busy else { return }
    let scanner = BeaconScanner()
    scanner.onScan = { [weak self] value in
      guard let self else { return }
      do {
        let invitation = try BeaconInvitation.parse(value)
        beaconURL.text = invitation.url
        code.text = invitation.code
        if invitation.code.isEmpty {
          status.text = "Beacon address scanned. Enter a current invitation to join."
        } else {
          joinBeacon()
        }
      } catch { status.text = "Unsupported QR: \(error)" }
    }
    present(UINavigationController(rootViewController: scanner), animated: true)
  }
  func acceptPairingLink(_ url: URL) {
    guard session != nil, !busy else {
      pendingLink = url
      return
    }
    do {
      let invitation = try BeaconInvitation.parse(url.absoluteString)
      selected = 3
      tabs.selectedItem = tabs.items?[3]
      if let item = tabs.selectedItem { tabBar(tabs, didSelect: item) }
      beaconURL.text = invitation.url
      code.text = invitation.code
      if !invitation.code.isEmpty {
        joinBeacon()
      } else {
        status.text = "Address received. Enter a current invitation to join."
      }
    } catch { status.text = "Unsupported pairing link: \(error)" }
  }
  @objc private func disconnectBeacon() {
    guard let session, !busy else { return }
    let alert = UIAlertController(
      title: "Disconnect beacon?", message: "Your local memories remain on this phone.",
      preferredStyle: .alert)
    alert.addAction(UIAlertAction(title: "Cancel", style: .cancel))
    alert.addAction(
      UIAlertAction(title: "Disconnect", style: .destructive) { _ in
        Task {
          do {
            try await session.disconnect()
            await self.refreshBeacon()
            self.status.text = "Beacon disconnected."
          } catch { self.status.text = String(describing: error) }
        }
      })
    present(alert, animated: true)
  }
  @objc private func connectMac() {
    let picker = UIDocumentPickerViewController(forOpeningContentTypes: [.json], asCopy: true)
    picker.delegate = self
    present(picker, animated: true)
  }
  func documentPicker(_ controller: UIDocumentPickerViewController, didPickDocumentsAt urls: [URL])
  {
    guard let url = urls.first, let session else { return }
    setBusy(true)
    Task {
      do {
        let scoped = url.startAccessingSecurityScopedResource()
        defer { if scoped { url.stopAccessingSecurityScopedResource() } }
        let values = try url.resourceValues(forKeys: [.fileSizeKey])
        try require((values.fileSize ?? 0) < 16384, "Pairing file is too large")
        let data = try Data(contentsOf: url)
        _ = try await session.pair(data: data)
        status.text = "Legacy pairing imported."
        await refreshBeacon()
      } catch { status.text = "Pairing failed: \(error)" }
      setBusy(false)
    }
  }
  @objc private func syncNow() {
    guard let session, !busy else { return }
    setBusy(true)
    status.text = "Syncing with your Memory beacon…"
    Task {
      do {
        let result = try await session.sync()
        status.text = "Synced: \(result.uploaded) uploaded, \(result.downloaded) downloaded."
      } catch { status.text = "Sync waiting: \(error)" }
      setBusy(false)
    }
  }
  @objc private func diagnostics() {
    let c = DiagnosticsController()
    c.title = "Diagnostics"
    c.navigationItem.leftBarButtonItem = UIBarButtonItem(
      title: "Done", style: .done, target: self, action: #selector(closeDiagnostics))
    present(UINavigationController(rootViewController: c), animated: true)
    (UIApplication.shared.delegate as? AppDelegate)?.start(c)
  }
  @objc private func closeDiagnostics() { dismiss(animated: true) }
}

private final class BeaconScanner: UIViewController, AVCaptureMetadataOutputObjectsDelegate {
  var onScan: ((String) -> Void)?
  private let capture = AVCaptureSession(), queue = DispatchQueue(label: "memory-fabric.qr-camera")
  private var found = false, preview: AVCaptureVideoPreviewLayer?
  override func viewDidLoad() {
    super.viewDidLoad()
    title = "Scan beacon QR"
    view.backgroundColor = .black
    navigationItem.leftBarButtonItem = UIBarButtonItem(
      title: "Cancel", style: .plain, target: self, action: #selector(cancel))
  }
  override func viewDidAppear(_ animated: Bool) {
    super.viewDidAppear(animated)
    switch AVCaptureDevice.authorizationStatus(for: .video) {
    case .authorized: start()
    case .notDetermined:
      AVCaptureDevice.requestAccess(for: .video) { granted in
        DispatchQueue.main.async {
          if granted && self.presentingViewController != nil {
            self.start()
          } else {
            self.showError()
          }
        }
      }
    default: showError()
    }
  }
  private func showError() {
    let label = UILabel()
    label.text =
      "Camera access is unavailable. You can paste the pairing link or enter the beacon URL and invitation in Devices."
    label.numberOfLines = 0
    label.textColor = .white
    label.textAlignment = .center
    label.translatesAutoresizingMaskIntoConstraints = false
    view.addSubview(label)
    NSLayoutConstraint.activate([
      label.centerYAnchor.constraint(equalTo: view.centerYAnchor),
      label.leadingAnchor.constraint(equalTo: view.leadingAnchor, constant: 25),
      label.trailingAnchor.constraint(equalTo: view.trailingAnchor, constant: -25),
    ])
  }
  private func start() {
    guard let camera = AVCaptureDevice.default(for: .video),
      let input = try? AVCaptureDeviceInput(device: camera), capture.canAddInput(input)
    else {
      showError()
      return
    }
    capture.addInput(input)
    let output = AVCaptureMetadataOutput()
    guard capture.canAddOutput(output) else {
      showError()
      return
    }
    capture.addOutput(output)
    output.setMetadataObjectsDelegate(self, queue: .main)
    output.metadataObjectTypes = [.qr]
    let layer = AVCaptureVideoPreviewLayer(session: capture)
    layer.videoGravity = .resizeAspectFill
    layer.frame = view.bounds
    view.layer.insertSublayer(layer, at: 0)
    preview = layer
    queue.async { self.capture.startRunning() }
  }
  override func viewDidLayoutSubviews() {
    super.viewDidLayoutSubviews()
    preview?.frame = view.bounds
  }
  override func viewWillDisappear(_ animated: Bool) {
    super.viewWillDisappear(animated)
    queue.async { self.capture.stopRunning() }
  }
  @objc private func cancel() { dismiss(animated: true) }
  func metadataOutput(
    _ output: AVCaptureMetadataOutput, didOutput metadataObjects: [AVMetadataObject],
    from connection: AVCaptureConnection
  ) {
    guard !found,
      let value = (metadataObjects.first as? AVMetadataMachineReadableCodeObject)?.stringValue
    else { return }
    found = true
    queue.async { self.capture.stopRunning() }
    dismiss(animated: true) { self.onScan?(value) }
  }
}

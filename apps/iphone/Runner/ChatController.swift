import UIKit
import UniformTypeIdentifiers
import FabricCore

final class ChatController:UIViewController,UITableViewDataSource,UIDocumentPickerDelegate {
    private let table=UITableView(frame:.zero,style:.plain)
    private let input=UITextView()
    private let send=UIButton(type:.system)
    private let status=UILabel()
    private let syncStatus=UILabel()
    private var syncDisplay:Task<Void,Never>?
    private var session:PhoneSession?
    private var turns:[SyncClient.Turn]=[]
    private var busy=false
    private var sending:Task<Void,Never>?
    private lazy var connect=UIBarButtonItem(title:"Connect Mac",style:.plain,target:self,action:#selector(connectMac))
    private lazy var syncButton=UIBarButtonItem(title:"Sync",style:.plain,target:self,action:#selector(syncNow))
    override func viewDidLoad(){
        super.viewDidLoad();title="Memory Fabric";view.backgroundColor = .systemBackground
        navigationItem.leftBarButtonItem=connect
        navigationItem.rightBarButtonItems=[UIBarButtonItem(title:"Diagnostics",style:.plain,target:self,action:#selector(diagnostics)),syncButton]
        status.text="Loading local memory…";status.font = .preferredFont(forTextStyle:.caption1);status.numberOfLines=0;status.textColor = .secondaryLabel
        syncStatus.font = .preferredFont(forTextStyle:.caption2);syncStatus.numberOfLines=0;syncStatus.textColor = .secondaryLabel
        table.dataSource=self;table.separatorStyle = .none;table.keyboardDismissMode = .interactive;table.backgroundColor = .systemGroupedBackground
        table.register(UITableViewCell.self,forCellReuseIdentifier:"turn");table.rowHeight=UITableView.automaticDimension;table.estimatedRowHeight=100
        input.font = .preferredFont(forTextStyle:.body);input.layer.cornerRadius=10;input.backgroundColor = .secondarySystemBackground
        input.accessibilityLabel="Prompt";input.textContainerInset=UIEdgeInsets(top:10,left:8,bottom:10,right:8)
        send.setTitle("Send",for:.normal);send.titleLabel?.font = .preferredFont(forTextStyle:.headline)
        send.addTarget(self,action:#selector(submit),for:.touchUpInside)
        let hint=UILabel();hint.text="LFM3 answers on this iPhone using local memory. Pair your Mac only to sync memories."
        hint.font = .preferredFont(forTextStyle:.caption2);hint.textColor = .secondaryLabel;hint.numberOfLines=0
        let row=UIStackView(arrangedSubviews:[input,send]);row.spacing=10;row.alignment = .bottom
        send.setContentHuggingPriority(.required,for:.horizontal);send.widthAnchor.constraint(equalToConstant:56).isActive=true
        input.heightAnchor.constraint(equalToConstant:88).isActive=true
        let composer=UIStackView(arrangedSubviews:[hint,row]);composer.axis = .vertical;composer.spacing=8
        for child in [status,syncStatus,table,composer]{child.translatesAutoresizingMaskIntoConstraints=false;view.addSubview(child)}
        NSLayoutConstraint.activate([
            status.topAnchor.constraint(equalTo:view.safeAreaLayoutGuide.topAnchor,constant:8),status.leadingAnchor.constraint(equalTo:view.leadingAnchor,constant:16),status.trailingAnchor.constraint(equalTo:view.trailingAnchor,constant:-16),
            syncStatus.topAnchor.constraint(equalTo:status.bottomAnchor,constant:4),syncStatus.leadingAnchor.constraint(equalTo:status.leadingAnchor),syncStatus.trailingAnchor.constraint(equalTo:status.trailingAnchor),
            table.topAnchor.constraint(equalTo:syncStatus.bottomAnchor,constant:8),table.leadingAnchor.constraint(equalTo:view.leadingAnchor),table.trailingAnchor.constraint(equalTo:view.trailingAnchor),
            table.bottomAnchor.constraint(equalTo:composer.topAnchor,constant:-10),composer.leadingAnchor.constraint(equalTo:view.leadingAnchor,constant:16),composer.trailingAnchor.constraint(equalTo:view.trailingAnchor,constant:-16),
            composer.bottomAnchor.constraint(equalTo:view.keyboardLayoutGuide.topAnchor,constant:-12)])
        NotificationCenter.default.addObserver(self,selector:#selector(suspendGeneration),name:UIApplication.willResignActiveNotification,object:nil)
        NotificationCenter.default.addObserver(self,selector:#selector(resumeSync),name:UIApplication.didBecomeActiveNotification,object:nil)
        setBusy(true)
        Task {
            do {
                guard let assets=Bundle.main.url(forResource:"DemoAssets",withExtension:nil) else {throw FabricError("App model assets are missing")}
                let storage=FileManager.default.urls(for:.documentDirectory,in:.userDomainMask)[0].appendingPathComponent("SharedMemory")
                let loaded=try await Task.detached(priority:.userInitiated){try PhoneSession(assets:assets,storage:storage)}.value
                session=loaded;turns=await loaded.conversation();table.reloadData()
                if UIApplication.shared.applicationState == .active { resumeSync() }
                status.text=await loaded.isPaired() ? "LFM3 available on-device · Mac paired for memory sync." : "LFM3 available on-device. Chat works without pairing."
            }catch{status.text="Could not open local memory: \(error.localizedDescription)"}
            setBusy(false)
        }
    }
    private func setBusy(_ value:Bool){busy=value;send.isEnabled = !value && session != nil;connect.isEnabled = !value && session != nil;syncButton.isEnabled = !value && session != nil}
    func tableView(_ tableView:UITableView,numberOfRowsInSection section:Int)->Int{turns.count}
    func tableView(_ tableView:UITableView,cellForRowAt indexPath:IndexPath)->UITableViewCell{
        let cell=tableView.dequeueReusableCell(withIdentifier:"turn",for:indexPath);cell.selectionStyle = .none;cell.backgroundColor = .clear
        var content=cell.defaultContentConfiguration();let turn=turns[indexPath.row]
        content.text=turn.role=="user" ? "You":"Assistant";content.textProperties.font = .preferredFont(forTextStyle:.caption1)
        content.textProperties.color=turn.role=="user" ? .systemGreen:.secondaryLabel
        content.secondaryText=turn.content;content.secondaryTextProperties.font = .preferredFont(forTextStyle:.body);content.secondaryTextProperties.numberOfLines=0
        content.directionalLayoutMargins=NSDirectionalEdgeInsets(top:12,leading:18,bottom:12,trailing:18);cell.contentConfiguration=content;return cell
    }
    private func scrollToEnd(){if !turns.isEmpty{table.scrollToRow(at:IndexPath(row:turns.count-1,section:0),at:.bottom,animated:true)}}
    @objc private func submit(){
        guard !busy,let session else{return}
        let question=input.text.trimmingCharacters(in:.whitespacesAndNewlines);guard !question.isEmpty else{return}
        setBusy(true);status.text="Loading LFM3 and answering on this iPhone…";input.resignFirstResponder()
        turns.append(SyncClient.Turn(role:"user",content:question));table.reloadData();scrollToEnd()
        sending=Task {
            do {
                let answer=try await session.send(question)
                turns=await session.conversation();input.text="";table.reloadData();scrollToEnd();status.text="Answered by \(answer.model)."
            }catch{
                turns=await session.conversation();table.reloadData();status.text="Could not answer locally: \(error). Your prompt is kept above."
            }
            setBusy(false)
        }
    }
    @objc private func resumeSync(){
        guard let session else { return }
        syncDisplay?.cancel()
        syncDisplay=Task { [weak self] in
            await session.startAutomaticSync()
            while !Task.isCancelled {
                let message=await session.automaticSyncStatus()
                guard !Task.isCancelled else { return }
                self?.syncStatus.text=message
                do { try await Task.sleep(nanoseconds:2_000_000_000) } catch { return }
            }
        }
    }
    deinit { syncDisplay?.cancel(); sending?.cancel() }
    @objc private func suspendGeneration(){
        syncDisplay?.cancel();syncDisplay=nil
        if let session { Task { await session.stopAutomaticSync() } }
        sending?.cancel()
        if let session {Task{await session.releaseGenerator()}}
    }
    @objc private func connectMac(){
        let picker=UIDocumentPickerViewController(forOpeningContentTypes:[.json],asCopy:true);picker.delegate=self;present(picker,animated:true)
    }
    func documentPicker(_ controller:UIDocumentPickerViewController,didPickDocumentsAt urls:[URL]){
        guard let url=urls.first,let session else{return};setBusy(true);status.text="Pairing with your Mac…"
        Task {
            do {
                let scoped=url.startAccessingSecurityScopedResource();defer{if scoped{url.stopAccessingSecurityScopedResource()}}
                let data=try Data(contentsOf:url);try require(data.count<16384,"Pairing file is too large")
                let result=try await session.pair(data:data);status.text="Mac connected. Synced \(result.downloaded) memory events. Replies still run on this iPhone."
            }catch{status.text="Pairing failed: \(error)"}
            setBusy(false)
        }
    }
    @objc private func syncNow(){
        guard let session,!busy else{return};setBusy(true);status.text="Syncing with private Git on your Mac…"
        Task{
            do{let result=try await session.sync();status.text="Git sync complete: \(result.uploaded) uploaded, \(result.downloaded) downloaded."}
            catch{status.text="Sync failed: \(error)"};setBusy(false)
        }
    }
    @objc private func diagnostics(){
        let controller=DiagnosticsController();controller.title="Diagnostics"
        controller.navigationItem.leftBarButtonItem=UIBarButtonItem(title:"Done",style:.done,target:self,action:#selector(closeDiagnostics))
        present(UINavigationController(rootViewController:controller),animated:true)
        (UIApplication.shared.delegate as? AppDelegate)?.start(controller)
    }
    @objc private func closeDiagnostics(){dismiss(animated:true)}
}

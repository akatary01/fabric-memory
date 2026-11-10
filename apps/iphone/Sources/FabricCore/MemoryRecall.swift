import Foundation

// Precision first: never feed generated chat or an entire memory collection back to the model.
enum MemoryRecall {
    static func select(_ hits:[Hit])->[Hit] {
        var selected:[Hit]=[];var seen=Set<String>();var remaining=800;var best:Float?
        for hit in hits.sorted(by:{$0.score>$1.score}) {
            let text=hit.text.trimmingCharacters(in:.whitespacesAndNewlines),lower=text.lowercased()
            let chat=hit.memory.hasPrefix("mac-chat-") || hit.memory.hasPrefix("chat-")
            let question=chat && (text.hasSuffix("?") || ["is ","are ","was ","were ","do ","did ","does ","would ","should ","what ","who ","why ","when ","where ","how ","can you ","could you ","check ","fetch ","search ","tell me "].contains(where:{lower.hasPrefix($0)}))
            guard hit.score.isFinite,hit.score>=0.50,!text.isEmpty,!(chat && hit.memory.hasSuffix("-assistant")),!question else{continue}
            if let best,best-hit.score>0.08{continue}
            let key=lower.split(whereSeparator:{$0.isWhitespace}).joined(separator:" ")
            guard seen.insert(key).inserted else{continue}
            if best==nil{best=hit.score}
            let clipped=String(text.prefix(min(400,remaining)));remaining-=clipped.count
            selected.append(Hit(memory:hit.memory,text:clipped,score:hit.score,origin:hit.origin,access:hit.access))
            if selected.count==2 || remaining==0{break}
        }
        return selected
    }
}

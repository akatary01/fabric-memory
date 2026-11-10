"""Short-lived, single-use room invitations and revocable device credentials."""
import hashlib
import secrets
import sqlite3
import time
from contextlib import contextmanager
from pathlib import Path

class Enrollment:
    def __init__(self,root,public_url):
        self.path=Path(root)/"devices.sqlite";self.public_url=public_url.rstrip("/")
        with self.db() as db:
            db.executescript("CREATE TABLE IF NOT EXISTS invitations(digest TEXT PRIMARY KEY,expires INTEGER NOT NULL,used INTEGER NOT NULL DEFAULT 0); CREATE TABLE IF NOT EXISTS devices(id TEXT PRIMARY KEY,digest TEXT UNIQUE NOT NULL,name TEXT NOT NULL,created INTEGER NOT NULL,revoked INTEGER NOT NULL DEFAULT 0);")
        with self.db() as db:db.execute("CREATE TABLE IF NOT EXISTS memory_owners(memory TEXT PRIMARY KEY,device TEXT NOT NULL)")
        self.path.chmod(0o600)
    @contextmanager
    def db(self):
        connection=sqlite3.connect(self.path,timeout=10);connection.execute("PRAGMA busy_timeout=10000")
        try:
            with connection:yield connection
        finally:connection.close()
    @staticmethod
    def digest(token):return hashlib.sha256(token.encode()).hexdigest()
    def authenticated(self,token):
        if not 32<=len(token)<=256:return False
        with self.db() as db:return db.execute("SELECT 1 FROM devices WHERE digest=? AND revoked=0",(self.digest(token),)).fetchone() is not None
    def invite(self):
        token=secrets.token_urlsafe(24);expires=int(time.time())+300
        with self.db() as db:
            db.execute("DELETE FROM invitations WHERE expires<?",(int(time.time())-3600,))
            db.execute("INSERT INTO invitations(digest,expires) VALUES(?,?)",(self.digest(token),expires))
        return {"url":self.public_url,"invitation":token,"expires_at":expires,"uri":self.public_url+"#join="+token}
    def join(self,invitation,name):
        if not isinstance(invitation,str) or not 20<=len(invitation)<=256:raise ValueError("Invalid or expired invitation")
        if not isinstance(name,str) or not 1<=len(name.strip())<=80:raise ValueError("Choose a device name of 1–80 characters")
        now=int(time.time());credential=secrets.token_urlsafe(32);device=secrets.token_hex(16)
        with self.db() as db:
            db.execute("BEGIN IMMEDIATE")
            row=db.execute("SELECT expires,used FROM invitations WHERE digest=?",(self.digest(invitation),)).fetchone()
            if row is None or row[1] or row[0]<=now:raise ValueError("Invalid, expired, or already-used invitation. Generate a new QR on the beacon.")
            db.execute("UPDATE invitations SET used=1 WHERE digest=?",(self.digest(invitation),))
            db.execute("INSERT INTO devices(id,digest,name,created) VALUES(?,?,?,?)",(device,self.digest(credential),name.strip(),now))
        return {"version":2,"type":"memory-fabric-pairing","url":self.public_url,"token":credential,"device_id":device,"device_name":name.strip()}
    def devices(self):
        with self.db() as db:return [{"id":r[0],"name":r[1],"created":r[2],"revoked":bool(r[3])} for r in db.execute("SELECT id,name,created,revoked FROM devices ORDER BY created DESC")]
    def revoke(self,device):
        with self.db() as db:db.execute("UPDATE devices SET revoked=1 WHERE id=?",(device,))

    def identity(self,token):
        if not isinstance(token,str) or not 32<=len(token)<=256:return None
        with self.db() as db:
            row=db.execute("SELECT id FROM devices WHERE digest=? AND revoked=0",(self.digest(token),)).fetchone()
            return row[0] if row else None
    def owner(self,memory):
        with self.db() as db:
            row=db.execute("SELECT device FROM memory_owners WHERE memory=?",(memory,)).fetchone()
            return row[0] if row else None
    def claim(self,memory,device):
        with self.db() as db:db.execute("INSERT OR IGNORE INTO memory_owners(memory,device) VALUES(?,?)",(memory,device))

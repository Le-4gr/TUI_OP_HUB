-- Secrets v2 (US-SEC): groups, password-manager style metadata fields,
-- optional passphrase wrapping for high-value secrets, and ssh-agent loading.
ALTER TABLE secrets ADD COLUMN secret_group TEXT;
ALTER TABLE secrets ADD COLUMN username TEXT;
ALTER TABLE secrets ADD COLUMN url TEXT;
ALTER TABLE secrets ADD COLUMN email TEXT;
-- When 1, value_enc holds user-key-encrypted( share_crypto::encrypt(plaintext, passphrase) )
-- and every use re-asks for that secret's own passphrase.
ALTER TABLE secrets ADD COLUMN passphrase_protected INTEGER NOT NULL DEFAULT 0;
-- When 1 (and secret_kind='ssh_key'), the key is offered to ssh-agent on login/startup.
ALTER TABLE secrets ADD COLUMN ssh_agent INTEGER NOT NULL DEFAULT 0;

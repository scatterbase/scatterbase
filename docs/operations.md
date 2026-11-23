# Running an instance

What an instance keeps on disk, and how to look after it: keys, backups, verification, erasure and recovery. The commands are described in [commands.md](commands.md) and the formats in the [specification](claim-envelopes.md).

## What an instance keeps

After `scatterbase init --default` and one `key add`, the working directory holds:

```text
.env                                  the configuration
data/log/                             the log: the source of truth
data/log/lock                         held by whichever command has the log open
data/log/0000000000000000/            the server partition
data/log/{16 hex digits}/             the claim partition and the mutable partition
    00000000.seg                      the records
    partition.cbor                    the partition's layout
    checkpoints/{size}.txt            a signed checkpoint for each size the partition has had
data/scatterbase.nq                   the store: derived from the log
data/scatterbase.nq.offset            how far into the log the store has got
data/prefixes.ttl                     the prefixes URIs may be written with
keys/scatterbase.ed25519.private      the server's private key
keys/scatterbase.ed25519.public       the server's public key
keys/actors/{hash}.ed25519.private    the private key of each actor `key add` generated one for
```

| What | If it is lost |
|---|---|
| The log | The instance's history is gone. Restore it from a backup bundle. |
| The server's private key | The log can still be read and verified, but nothing more can be accepted into it. |
| An actor's private key | That actor can no longer claim. Revoke the key and register a new one. |
| The store and its position | Nothing is lost: `scatterbase log rebuild` writes them again. |
| The configuration | Write it again by hand; it holds paths and names, and no secrets. |

**The private keys are the secrets.** They are written readable by their owner only. Everything else can be read by anyone the instance's data is for.

### The configuration file

`.env` holds one setting to a line:

| Setting | Meaning |
|---|---|
| `DOMAIN` | The instance's domain: the base of its graph IRIs and of the IRIs given to blank nodes, and the host its checkpoints are signed under |
| `SERVER_URI` | The URI the server claims and attests as |
| `STORAGE_FILE` | The store's N-Quads file |
| `LOG_DIR` | The log's directory. Optional; the default is `log` beside the storage file. |
| `ED25519_SECRET_KEY_FILE` | The server's private key file. Actors' keys are kept in `actors/` beside it. |
| `ED25519_PUBLIC_KEY_FILE` | Where the server's public key is written |
| `PREFIX_FILE` | The prefix file |

A setting that is already in the environment when a command starts takes precedence over the file.

## Keys

### Actors' keys

An actor can claim once the server has registered a key for it:

```bash
scatterbase key add --attribution https://example.org/alice
```

This generates the key pair on the server's machine, which suits an actor the server acts for. An actor who holds their own key gives the server a private key file to read the public key from (`--file`), and signs with it themselves (`claim prepare --private-key`).

To replace a key, revoke it and add another:

```bash
scatterbase key revoke --attribution https://example.org/alice
scatterbase key add --attribution https://example.org/alice
```

Revoking a key does not undo what it signed. To find the ID of one key among several, use `scatterbase key list --attribution <actor>`.

### The server's key

The server's key signs every checkpoint and every acceptance, so everything rests on it.

**Publish the public key.** Anyone who verifies a copy of the log needs the server's public key from somewhere they trust, not from the copy itself. `scatterbase key list` prints it as `urn:ed25519:…`.

**Rotating it** replaces it with a new key that the old one vouches for:

```bash
scatterbase key rotate
```

After a rotation only the new key signs. What the old key signed still verifies, because the log remembers every key the server has had. Take note before rotating:

- **The old private key is overwritten.** There is no going back to it.
- **Publish the new public key.** Readers who verify against the old one will be told the log's current key is not the one they expected.
- **Rotation is not a defence against theft.** Whoever holds the current private key can rotate to a key of their own. If the key may have been stolen, rotating does not make the log trustworthy again.

## Backups and copies

A **bundle** is a copy of the log in a directory, which verifies on its own.

```bash
scatterbase log export /backups/2026-11-23 --backup
```

A backup holds everything: all three partitions, with the records that say who the actors are. Keep it as you would the log. It does **not** hold the private keys or the configuration; back those up separately, and more carefully.

To check a backup, anywhere:

```bash
scatterbase log verify --bundle /backups/2026-11-23 --key urn:ed25519:<the server's public key>
```

**Restoring.** There is no restore command. A backup bundle has the layout of a log directory, so restoring is copying it into place and rebuilding the store:

```bash
cp -R /backups/2026-11-23 data/log
scatterbase log rebuild
scatterbase log verify
```

The bundle's own files, `bundle.cbor` and `keys.seg`, are ignored by the log and can be left or removed.

**Publishing.** Without `--backup`, a bundle leaves out the mutable partition, and so the records of who the actors are and what keys they have. It still holds every claim, and each claim still names its actor unless that was erased.

```bash
scatterbase log export ./public
```

## Verifying

```bash
scatterbase log verify
```

This reads the whole log and checks that:

- the server's keys form an unbroken chain from the founding record, each signed by the one before;
- every partition's records hash to the tree head its latest checkpoint signs, and the checkpoint is signed by one of the server's keys;
- every record's body is the one its header commits to, and every missing part is covered by a record of its erasure;
- every claim is the claim its ID names, was accepted under one of the server's keys, and, where nothing of it was erased, carries its actor's signature.

It holds the log's lock while it runs, so other commands wait for it.

If it fails, the message names the partition and usually the offset. The log is not repaired automatically. Restore from a backup that verifies.

## Erasing

Erasure is permanent, and it is recorded. Decide what to erase before running anything.

**What can go, and what it costs:**

| Erase | When | Cost |
|---|---|---|
| `content` | What was claimed must go | The claim can no longer be checked against its ID or its actor's signature |
| `attestation` | Who claimed it must go | The claim is nobody's: it leaves its actor's chain, and the actor's next claim follows an earlier one |
| `acknowledgments` | The terms it used must go | The claim is no longer found as the last use of those terms |

Whatever is erased, the claim's ID, its place in the log, the time it was accepted and the server's signature remain. That is by design: later claims point at it.

**To remove a person:**

1. Find their claims. Each is a `prov:wasAttributedTo` triple in the store's `meta` graph.
2. Erase the attestation of each, and the content of any whose triples identify them:
   ```bash
   scatterbase claim erase <claim> --parts attestation --reason privacy --authority ticket:4411
   ```
3. Erase the actor's record and keys:
   ```bash
   scatterbase actor erase --attribution https://example.org/alice --reason privacy --authority ticket:4411
   ```
4. Verify, and replace old backups and published bundles, which still hold what was erased:
   ```bash
   scatterbase log verify
   ```

The reason class and the authority are kept in the log, in the record of the erasure, with a public comment if one is given. They are how someone verifying the log later can tell an erasure from damage.

**Erasure does not reach copies.** A bundle exported earlier, a backup, or anyone's copy of the store still holds what was erased.

## When something is interrupted

Commands are written so that stopping one part-way leaves the instance usable.

| Interrupted | What is left | What to do |
|---|---|---|
| Any command, while it holds the lock | Nothing: the lock is the operating system's and goes when the process does | Run the command again |
| `claim write` or `claim submit`, after the claim reached the log | The store is behind the log | Nothing: the next command that writes brings the store up to the log |
| `key add`, before the log registered the key | A `.next` file beside the actor's key file | Run `key add` again; it writes a new one |
| `key rotate`, after the log recorded the new key | The new key in `{server key file}.next`, the old key still in place, and commands failing because the signing key is not the log's | Run `key rotate` again; it finishes the rotation |
| `init` | Key files and a log, but no configuration file | Remove `data/` and `keys/` and run `init` again |
| `log rebuild` | A partly written store | Run `log rebuild` again |

If a command reports that the log "is in use by another process" after waiting, another command still has it open: most likely a long `log verify`. Wait for it.

## Starting again

```bash
scatterbase purge
```

deletes the log, the store, every key, the prefix file and the configuration. Nothing can be recovered from the instance afterwards, and without the server's private key nothing more can ever be appended to a copy of its log.

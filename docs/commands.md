# Commands

Every command of the `scatterbase` binary. Each also describes itself: `scatterbase <command> --help`.

## Conventions

- **The instance.** Every command but `init` and `log verify --bundle` acts on an instance that `init` set up, and finds it through its configuration file. `--env <path>` names a configuration file other than `.env`; it can be given anywhere on the line.
- **URIs** must be absolute. They may be written with or without angle brackets around them.
- **Attribution.** Every claim names who makes it with `--attribution` (`-a`): an actor whose key `key add` registered, or the server's own URI. There is no default.
- **Output and status.** Results go to standard output. A command that fails says why on standard error and exits with status 1; a command line that cannot be understood exits with status 2. A command whose output is piped to a reader that stops reading, such as `head`, ends quietly.
- **One at a time.** Commands that touch the log wait for one another, for up to half a minute.

## `init`

Sets up a new instance in the current directory: its configuration file, the server's key pair, its log with the first claim, and its prefix file.

```bash
scatterbase init            # asks for the domain, file paths and server URI
scatterbase init --default  # asks nothing
```

| Option | Meaning |
|---|---|
| `-d`, `--default` | Ask nothing and use the defaults: a generated `http://{16 letters}.sandbox` domain, `data/scatterbase.nq`, `{domain}/Server`, `keys/scatterbase.ed25519.private` |

The first claim is the server's declaration that the domain is a `schema:Dataset`. `init` refuses to run where the configuration file already exists; use `purge` first to start again.

## `claim`

### `claim write`

Prepares a claim and accepts it, in one step.

```bash
scatterbase claim write --attribution https://example.org/alice --file triples.ttl
cat triples.nt | scatterbase claim write -a https://example.org/alice
```

| Option | Meaning |
|---|---|
| `-a`, `--attribution <URI>` | Who makes the claim. Required. |
| `-f`, `--file <FILE>` | The triples: Turtle if the name ends in `.ttl`, N-Triples otherwise. Without it, standard input is read as Turtle, which N-Triples is a subset of. |
| `-d`, `--domain <URI>` | The domain blank nodes are given IRIs under. Defaults to the instance's. |
| `-k`, `--private-key <FILE>` | The file of the key to sign with. Defaults to the actor's key file, which `key add` writes, or the server's key for the server. |

It prints the claim's ID, its sequence ID, the time of acceptance and the server's signature. Claims written at the same moment are all accepted, one after another.

Blank nodes become IRIs under the domain, and a quoted triple (`<< s p o >>`) becomes the IRI of that triple; see the [specification](claim-envelopes.md#blank-nodes-and-quoted-triples). A URI written with a prefix from the prefix file, as `<schema:name>`, is expanded.

### `claim prepare`

Writes the signed envelope of a claim to standard output, as Turtle, without submitting it. It takes the same options as `claim write`.

```bash
scatterbase claim prepare -a https://example.org/alice -f triples.ttl > envelope.ttl
```

### `claim submit`

Verifies a prepared envelope and appends its claim to the log.

```bash
scatterbase claim submit --file envelope.ttl
scatterbase claim prepare -a https://example.org/alice -f triples.ttl | scatterbase claim submit
```

| Option | Meaning |
|---|---|
| `-f`, `--file <FILE>` | The envelope. Without it, standard input is read. |

An envelope is refused if:

- its actor has no registered key, or its signature does not verify against one;
- the log already holds the claim;
- it is out of date: another claim by the same actor, or about one of the same terms, was accepted after it was prepared. Prepare it again.

### `claim erase`

Erases parts of an accepted claim, for good.

```bash
scatterbase claim erase <claim> --parts content --reason privacy --authority ticket:4411
scatterbase claim erase <claim> --parts content,attestation,acknowledgments \
    --reason legal --authority notice:7 --comment "court order"
```

| Option | Meaning |
|---|---|
| `<claim>` | The claim's ID |
| `--parts <PARTS>` | What to erase, separated by commas: `content` (what was claimed), `attestation` (who claimed it, with their signature and key), `acknowledgments` (which terms it used and what it had seen of them). Required. |
| `--reason <CLASS>` | `legal`, `privacy`, `upstream` or `operational`. Required. |
| `--authority <TEXT>` | What authorizes the erasure, such as a ticket or a request. Required. |
| `--comment <TEXT>` | A public reason, kept in the record of the erasure |

The claim keeps its ID, its place in the log and the server's signature. **This cannot be undone.** Read [Erasure](operations.md#erasing) first.

## `triple write`

Claims one triple. It is `claim write` for a triple given on the command line.

```bash
scatterbase triple write https://example.org/ada http://schema.org/name '"Ada Lovelace"' \
    --attribution https://example.org/alice
```

| Argument or option | Meaning |
|---|---|
| `<subject>`, `<property>` | URIs |
| `<object>` | A URI, or a literal enclosed in double quotes |
| `-a`, `--attribution <URI>` | Who makes the claim. Required. |
| `-k`, `--private-key <FILE>` | The file of the key to sign with |

## `domain init`

Claims, as the server, that a domain is a `schema:Dataset`.

```bash
scatterbase domain init https://example.net
```

Without a URI it asks for one.

## `key`

### `key add`

Registers a key for an actor, so that claims attributed to it are accepted. The registration is a record the server writes to the log.

```bash
scatterbase key add --attribution https://example.org/alice
scatterbase key add --attribution https://example.org/bob --file bob.private
scatterbase key add --attribution https://example.org/carol --expires 2027-12-31T00:00:00Z
```

| Option | Meaning |
|---|---|
| `-a`, `--attribution <URI>` | The actor. Required. |
| `-f`, `--file <FILE>` | The file of a private key the actor already holds, whose public key is registered. Without it a new key pair is generated and its private key kept with the server's. |
| `--expires <TIME>` | When the key stops being valid, as `2027-12-31T00:00:00Z`. Without it the key does not expire. |

It refuses if the actor already has a key; revoke that one first. The server's own key is not registered this way: see `key rotate`.

### `key revoke`

Revokes the keys registered for an actor.

```bash
scatterbase key revoke --attribution https://example.org/alice
scatterbase key revoke --attribution https://example.org/alice --key-id <key ID>
```

| Option | Meaning |
|---|---|
| `-a`, `--attribution <URI>` | The actor. Required. |
| `--key-id <ID>` | The one key to revoke. Without it, all of the actor's keys. |

A revoked key signs nothing more. The claims it signed stay in the log and still verify.

### `key list`

Lists the server's keys, or with `--attribution` the keys of an actor, each with its key ID, what has become of it (`current`, `replaced`, `revoked`, `expired`) and its public key.

```bash
scatterbase key list
scatterbase key list --attribution https://example.org/alice
```

### `key rotate`

Replaces the server's key with a new one.

```bash
scatterbase key rotate
```

The log records the new key, signed by the old one. What the old key signed still verifies. The old private key is overwritten. Read [The server's key](operations.md#the-servers-key) first.

## `actor erase`

Erases an actor from the log, for good: the record that says who the actor is, and every key registered for it.

```bash
scatterbase actor erase --attribution https://example.org/alice --reason privacy --authority ticket:4412
```

It takes `--reason`, `--authority` and `--comment` as `claim erase` does. The actor then has no keys. The claims it made are not touched; each still names the actor until its attestation is erased with `claim erase`. **This cannot be undone.**

## `log`

### `log verify`

Checks the whole log against the server's key: the chain of the server's keys, every signed checkpoint, every record, every signature, and that every erased part is accounted for by a record of its erasure.

```bash
scatterbase log verify
```

With `--bundle` it checks a bundle instead, and needs no instance:

```bash
scatterbase log verify --bundle ./bundle --key urn:ed25519:<the server's public key>
```

| Option | Meaning |
|---|---|
| `--bundle <DIRECTORY>` | Verify the bundle in this directory |
| `--key <KEY>` | The server's public key to verify the bundle against. Without it the bundle is checked against the key its own chain ends in, which shows that it is consistent but not whose it is. |

### `log rebuild`

Writes the storage file again from the log.

```bash
scatterbase log rebuild
```

### `log export`

Writes a bundle: a copy of the log that can be verified on its own.

```bash
scatterbase log export ./bundle
scatterbase log export ./backup --backup
```

| Argument or option | Meaning |
|---|---|
| `<directory>` | Where to write the bundle. It must not exist, or must be empty. |
| `--backup` | Also include the mutable partition, which says who the actors are and what keys they have. Not for publication. |

## `term latest`

Prints the ID of the claim that last used a term, as subject or object. If no claim has, it prints the ID of no claim, `yyyy…y`.

```bash
scatterbase term latest https://example.org/ada
```

## `prefix add`

Adds a prefix that URIs in files of triples may be written with.

```bash
scatterbase prefix add schema http://schema.org/
```

After this, `<schema:name>` in an N-Triples file is read as `<http://schema.org/name>`. Turtle files declare their own prefixes and do not need it.

## `purge`

Deletes everything the instance keeps: the log, the store, every key, the prefix file and the configuration file.

```bash
scatterbase purge        # asks for confirmation
scatterbase purge --yes
```

**This cannot be undone**, and the keys cannot be recovered.

# Moving the backups to the new server

Here's the plan for moving the nightly backups off the old box and onto the new one, with the
steps in the order I'd run them. Nothing here touches the live database until **step 4**, and
every step before it can be undone by deleting what it made.

## What changes

Today the backups run from a cron job on `old-box` at 02:30, write a compressed dump to the
local disk, and copy it to the storage bucket with a script nobody has read in a while. After
the move:

- the dump runs on `new-box`, from a *systemd timer* instead of cron, so a missed run (the box
  was off) runs as soon as it's back;
- the dump is streamed straight to the bucket, so it never sits on a local disk;
- each run writes a small report with its size, its checksum and how long it took;
- a failed run sends you one message, not one per retry.

The schedule stays at 02:30 local time. The retention stays at **35 daily** and **12 monthly**
copies.

## Before you start

You'll need three things at hand:

1. a shell on `new-box` with `sudo`;
2. the bucket's write key, which is in the password manager under *Backups / bucket writer*;
3. about forty minutes, most of it waiting for the first full dump.

> If the first dump takes much longer than the old one did (it's been about 11 minutes), stop
> and tell me before going on: it usually means the new box is reading over the slow network
> path, and the fix is a one-line change to the mount.

## Step 1: install the tools

On `new-box`, install the client and check its version:

```bash
sudo apt-get update
sudo apt-get install -y postgresql-client-16 zstd rclone
pg_dump --version
rclone version | head -n 1
```

You should see `pg_dump (PostgreSQL) 16.x` and an rclone of at least `v1.66`. An older rclone
can't stream uploads larger than 48 GB, and the dump is already 31 GB.

## Step 2: give it the bucket

Make a config for rclone that only the backup user can read:

```bash
sudo useradd --system --home /var/lib/backup --create-home backup
sudo -u backup rclone config create bucket s3 \
  provider=Other \
  endpoint=https://storage.example.net \
  access_key_id=BACKUP_WRITER \
  secret_access_key="$(read -rsp 'key: ' k; echo "$k")" \
  no_check_bucket=true
sudo chmod 600 /var/lib/backup/.config/rclone/rclone.conf
```

Then check that it can list the bucket but not delete from it:

```bash
sudo -u backup rclone lsd bucket:db-backups
sudo -u backup rclone deletefile bucket:db-backups/does-not-exist || echo "refused, as it should be"
```

The second command should fail with a permission error. If it *succeeds*, the key has more
rights than it needs: stop there, and I'll make a narrower one.

## Step 3: the job itself

Save this as `/usr/local/bin/nightly-backup` and make it executable:

```bash
#!/usr/bin/env bash
set -euo pipefail

stamp="$(date -u +%Y-%m-%dT%H%MZ)"
target="bucket:db-backups/daily/${stamp}.sql.zst"
started=$(date +%s)

pg_dump --format=plain --no-owner --dbname="$DATABASE_URL" \
  | zstd -T0 -10 \
  | tee >(sha256sum | cut -d' ' -f1 > /tmp/backup.sha256) \
  | rclone rcat "$target"

size=$(rclone size --json "$target" | jq .bytes)
took=$(( $(date +%s) - started ))
printf '{"file":"%s","bytes":%s,"sha256":"%s","seconds":%s}\n' \
  "$target" "$size" "$(cat /tmp/backup.sha256)" "$took" \
  | rclone rcat "bucket:db-backups/reports/${stamp}.json"
```

A few things worth knowing about it:

- `set -euo pipefail` makes the whole pipeline fail if *any* part of it fails, so a dump that
  dies halfway doesn't leave a short file that looks like a good one.
- `zstd -T0 -10` uses every core at a level that's about as fast as the network; going higher
  saves little and doubles the time.
- The checksum is taken from the same stream that's uploaded, so it describes exactly the bytes
  in the bucket.

Then the timer, in `/etc/systemd/system/nightly-backup.timer`:

```ini
[Unit]
Description=Nightly database backup

[Timer]
OnCalendar=*-*-* 02:30:00
Persistent=true
RandomizedDelaySec=120

[Install]
WantedBy=timers.target
```

`Persistent=true` is what makes a missed run happen when the box comes back.

## Step 4: the first run, by hand

Run it once while you watch, before the timer ever does:

```bash
sudo systemctl daemon-reload
sudo systemctl start nightly-backup.service
journalctl -u nightly-backup.service -f
```

When it finishes, compare it with last night's dump from the old box:

| | Old box | New box (expected) |
|---|---|---|
| Size | 4.1 GB | 3.6–4.2 GB |
| Time | about 11 min | 8–14 min |
| Format | gzip, level 6 | zstd, level 10 |
| Checksum file | none | `reports/<stamp>.json` |

A size more than 15% off either way is worth a look: much smaller usually means a table was
skipped, and much larger that compression didn't run.

## Step 5: turn the old one off

Only once the new run looks right:

1. on `old-box`, comment out the cron line rather than deleting it (`crontab -e`, then a `#` in
   front of the `30 2 * * *` line);
2. leave the old dumps where they are for now: they age out on their own after 35 days;
3. on `new-box`, enable the timer with `sudo systemctl enable --now nightly-backup.timer`.

Check the next morning that `reports/` has a new file with a sensible size, and that the
journal says the run *succeeded*:

```bash
systemctl list-timers nightly-backup.timer
journalctl -u nightly-backup.service --since yesterday | tail -n 5
```

## If something goes wrong

- **The upload fails with `SignatureDoesNotMatch`.** The clock on `new-box` is off. Check it
  with `timedatectl`; the fix is `sudo timedatectl set-ntp true`.
- **The dump fails with `permission denied for table`.** The database user is missing a grant.
  Run `GRANT pg_read_all_data TO backup;` as the database's owner, then start the service again.
- **It's much slower than expected.** Look at where the data directory is mounted
  (`findmnt /var/lib/postgresql`). If it's on the network share, that's the slow path I mentioned
  above.
- **Nothing ran at all.** `systemctl list-timers` shows the next and last runs; a timer that was
  never *enabled* shows no last run.

## Restoring, for when you need it

The restore is the dump in reverse, and it's worth trying once on a scratch database so we know
it works before it matters:

```bash
createdb restore_test
rclone cat "bucket:db-backups/daily/<stamp>.sql.zst" \
  | zstd -d \
  | psql --dbname=restore_test --set ON_ERROR_STOP=on
psql --dbname=restore_test -c 'select count(*) from orders;'
```

The count should match production's within the orders placed since the dump. When you're done,
`dropdb restore_test`.

---

That's everything. The two places where I'd most like you to stop and check with me are the
**permission test in step 2** and the **size comparison in step 4**; everything else is safe to
run through. Once it's been running for a week I'll look at the reports and suggest whether the
retention could be shorter, since the monthly copies are most of what the bucket costs. You can
also read more about the timer settings in the [systemd documentation](https://www.freedesktop.org/software/systemd/man/latest/systemd.timer.html).

## What it costs, and what we could change later

The bucket is billed for what it stores and for what's read back out of it. Writing is free,
which is why the job streams straight to it rather than keeping a local copy first. At today's
size the numbers work out like this:

| Copies | Count | Size each | Stored |
|---|---|---|---|
| Daily | 35 | about 3.9 GB | 137 GB |
| Monthly | 12 | about 3.9 GB | 47 GB |
| Reports | 47 | under 1 KB | nothing to speak of |
| **All** | | | **about 184 GB** |

That's roughly *four times* what the old box's disk could hold, which is the main reason the old
setup kept only seven days. A few things we could do once this has run for a while:

1. **Keep fewer dailies.** Fourteen days would cover every restore we've ever needed (the
   oldest was nine days back) and would cut the stored size by about 80 GB.
2. **Dump the big log table separately.** `request_log` is 61% of the dump and is only ever
   read for the last week; it could have its own, shorter retention.
3. **Move the monthlies to the archive tier.** Reading them back takes hours instead of seconds,
   but we've never restored a monthly copy, and the archive tier costs about a fifth as much.

None of these need deciding now. I'd rather see a few weeks of reports first, so we're choosing
from real sizes and times rather than from today's guess.

### On checking the copies

A backup nobody has restored is a hope, not a backup. Once a month, on the first Sunday, I'd
like to restore the newest daily copy to a scratch database and compare a handful of counts with
production:

- the number of `orders`, `customers` and `invoices`;
- the newest `created_at` in each of those, which should be within a minute of the dump's time;
- the size of the restored database, which should be within a few percent of production's.

I can run that check myself and send you the numbers, so all you'd see is a short message on the
first Sunday saying it passed (or, if it didn't, what was off). If you'd rather run it yourself,
the restore steps above are all it takes, plus these three queries:

```sql
select count(*) from orders;
select max(created_at) from orders;
select pg_size_pretty(pg_database_size(current_database()));
```

### Things I deliberately left out

- **Encrypting the dump before upload.** The bucket encrypts everything at rest and the key
  never leaves the backup user, so a second layer would mostly add a key we could lose. If the
  bucket ever moves to a provider we trust less, this is the first thing to add.
- **Point-in-time recovery.** Shipping the write-ahead log would let us restore to any minute,
  not just to 02:30. It's a bigger change, with its own disk and network costs, and nothing
  we've lost so far would have needed it.
- **Alerts for slow runs.** Only failed runs send a message for now. If a run takes more than
  twice as long as the week's average, that's worth knowing about too, and the report has the
  numbers to tell, but I'd like a few weeks of normal times before picking the threshold.

Let me know when you've finished step 4, or if anything along the way looks different from what
I've described, and I'll take it from there.

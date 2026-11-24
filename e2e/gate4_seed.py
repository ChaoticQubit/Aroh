import datetime
import sqlite3
import sys
import uuid
import zoneinfo

GOALS = {
    "Six pack by Feb 1": ("Six pack by Feb 1 with a stronger lower back and better posture at my standing desk",
                          "I sit nine hours a day, my lower back hurts after long meetings, and the last two attempts failed because I trained too hard in week one and then stopped for a month."),
    "Learn piano": ("Learn piano well enough to play Clair de Lune for my mother's seventieth birthday party",
                    "I played as a child until age eleven, can still read treble clef slowly, and have a digital piano with weighted keys in the living room."),
    "Save $10k": ("Save $10k for the emergency fund before the lease renewal in June without skipping the family holiday",
                  "Rent goes up in June. I want six months of fixed costs saved, and I keep overspending on takeaway on weekdays."),
}
READING = ("Read The Joy of X by Steven Strogatz cover to cover before the spring semester starts in February",
           "Twenty four chapters, about three hundred pages. I read best in the morning with coffee and lose the thread when I skip more than two days.")
EXTRA = [
    ("06:15", 20, "Mobility warm-up: cat-cow, bird dog, dead bug and ninety-ninety hip switches before the first call", "done"),
    ("07:30", 15, "Write tomorrow's lunch and dinner plan in the notes app and move the chicken from the freezer", "done"),
    ("08:45", 10, "Check https://www.example.org/progressive-overload-and-recovery-for-desk-workers-guide", "skipped"),
    ("10:00", 25, "Review last week's grocery receipts and tag every takeaway order in the budget spreadsheet", "missed"),
    ("12:30", 30, "Lunch walk around the lake with the long loop past the library and back through the park", "planned"),
    ("14:00", 15, "Hammer curls, farmer carries and a slow plank ladder in the office stairwell", "planned"),
    ("16:30", 20, "Scales in C, G and D major hands together at sixty beats per minute with the metronome", "planned"),
    ("18:15", 45, "Strength session B: goblet squats, Romanian deadlifts, push-ups and pallof press", "planned"),
    ("19:30", 30, "Clair de Lune bars one to fourteen, left hand alone first, then both hands slowly", "planned"),
    ("21:00", 20, "Read one chapter of The Joy of X and write a three sentence summary in the reading log", "planned"),
]


def iso(dt):
    """UTC ISO text as the store writes it. Input: aware datetime. Output: text like 2026-11-23T06:15:00Z."""
    return dt.astimezone(datetime.timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")


def main():
    """Turns the six-pack sample in the app store into long real-world data for the build 4 gate.
    Input: store path and IANA zone. Output: long goal, series and action titles, a reading goal,
    ten one-off actions today and three on each of the next six days with done, skipped and missed
    ones; prints the counts."""
    path, zone = sys.argv[1], zoneinfo.ZoneInfo(sys.argv[2])
    db = sqlite3.connect(path)
    db.row_factory = sqlite3.Row
    goals = {r["title"]: dict(r) for r in db.execute("SELECT * FROM goals WHERE deleted = 0")}
    assert all(t in goals for t in GOALS), f"sample goals missing: {list(goals)}"
    for old, (title, context) in GOALS.items():
        db.execute("UPDATE goals SET title = ?, context = ? WHERE id = ?", (title, context, goals[old]["id"]))
    template = dict(goals["Learn piano"])
    cat = db.execute("SELECT id FROM categories WHERE name LIKE '%Learn%' OR name LIKE '%Read%' OR name LIKE '%Education%' LIMIT 1").fetchone()
    reading = dict(template, id=str(uuid.uuid4()), title=READING[0], context=READING[1], deadline="2027-02-01", category_id=cat["id"] if cat else template["category_id"])
    db.execute(f"INSERT INTO goals ({', '.join(reading)}) VALUES ({', '.join('?' * len(reading))})", list(reading.values()))
    for r in db.execute("SELECT id, title FROM action_series WHERE deleted = 0").fetchall():
        db.execute("UPDATE action_series SET title = ? WHERE id = ?", (f"{r['title']} with the full warm-up and the cool-down stretch routine", r["id"]))
    db.execute("UPDATE actions SET title = title || ' with the full warm-up and the cool-down stretch routine' WHERE series_id IS NOT NULL")
    row = dict(db.execute("SELECT * FROM actions WHERE deleted = 0 LIMIT 1").fetchone())
    owner = {"budget": goals["Save $10k"]["id"], "receipts": goals["Save $10k"]["id"], "Scales": goals["Learn piano"]["id"],
             "Clair": goals["Learn piano"]["id"], "Joy of X": reading["id"]}
    pick = lambda title: next((g for k, g in owner.items() if k in title), goals["Six pack by Feb 1"]["id"])
    today = datetime.datetime.now(zone).date()
    count = 0
    for day in range(7):
        date = today + datetime.timedelta(days=day)
        picks = EXTRA if day == 0 else EXTRA[4 + day % 3: 7 + day % 3]
        for at, minutes, title, status in picks:
            start = datetime.datetime.combine(date, datetime.time.fromisoformat(at), zone)
            end = start + datetime.timedelta(minutes=minutes)
            done = iso(end) if status == "done" else None
            a = dict(row, id=str(uuid.uuid4()), goal_id=pick(title), series_id=None, occurrence=None, title=title, notes=None,
                     start_at=iso(start), end_at=iso(end), status=status if day == 0 else "planned", locked=0, done_at=done if day == 0 else None, deleted=0)
            if "card" in a:
                a["card"] = None
            db.execute(f"INSERT INTO actions ({', '.join(a)}) VALUES ({', '.join('?' * len(a))})", list(a.values()))
            count += 1
    db.commit()
    statuses = dict(db.execute("SELECT status, count(*) FROM actions WHERE deleted = 0 GROUP BY status").fetchall())
    print(f"seeded {count} actions, statuses {statuses}, goals {db.execute('SELECT count(*) FROM goals WHERE deleted = 0').fetchone()[0]}")
    assert count == 10 + 6 * 3, "ten actions today and three on each of six days"


if __name__ == "__main__":
    main()

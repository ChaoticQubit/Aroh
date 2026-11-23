import datetime
import json
import os
import sys
import zoneinfo

FMT = "%Y-%m-%dT%H:%M:%S"
TEMPLATE = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "crates", "aroh-plugins", "templates", "fitness.toml")
MUSIC = os.path.join(os.path.dirname(TEMPLATE), "music.toml")
MUSIC_GOAL = "Clair de Lune"


def plan_times(now, room=30):
    """First one-off slot and the series start, both local and inside 06:00 to 22:00.
    Input: aware local now and the minutes that must fit before 22:00. Output: (workout
    start, plank series start); the workout is 40 minutes out (rounded to 5), else 07:00 tomorrow."""
    start = (now + datetime.timedelta(minutes=40)).replace(second=0, microsecond=0)
    start = start.replace(minute=start.minute - start.minute % 5)
    day_start = now.replace(hour=6, minute=0, second=0, microsecond=0)
    start = max(start, day_start)
    if start + datetime.timedelta(minutes=room) > now.replace(hour=22, minute=0, second=0, microsecond=0):
        start = (now + datetime.timedelta(days=1)).replace(hour=7, minute=0, second=0, microsecond=0)
    plank = (now + datetime.timedelta(days=1)).replace(hour=6, minute=0, second=0, microsecond=0)
    return start, plank


def script(now, reminder_minutes):
    """The fake model script for the six-pack loop at local time `now`.
    Input: aware local now and minutes until the meals check-in reminder.
    Output: the script as a dict for `aroh_server::fake::Script`."""
    workout, plank = plan_times(now)
    with open(TEMPLATE, encoding="utf8") as f:
        bundle = f.read()
    meals_at = (now + datetime.timedelta(minutes=reminder_minutes)).strftime("%H:%M")
    zone = str(now.tzinfo)
    questions = {"questions": [
        {"id": "days", "text": "How many days a week can you train?", "kind": "number", "choices": []},
        {"id": "food", "text": "Anything you do not eat?", "kind": "text", "choices": []},
    ]}
    research = {"summary": "Core training three to four times a week plus a calorie deficit; abs show below about 12 percent body fat.",
                "sources": [{"title": "Core training basics", "url": "https://example.org/core"}]}
    plan = {
        "summary": "Short core workouts, a daily plank and a weight and meals check-in until Feb 1.",
        "timezone": zone,
        "items": [
            {"title": "Core workout", "start": workout.strftime(FMT), "minutes": 30, "rrule": None, "notes": "Crunches, leg raises, side planks"},
            {"title": "Morning plank", "start": plank.strftime(FMT), "minutes": 15, "rrule": "FREQ=DAILY", "notes": None},
        ],
        "trackers": [
            {"name": "Weight", "fields": [{"key": "weight", "label": "Weight", "kind": "number", "unit": "kg"}],
             "cadence": {"freq": "daily", "weekdays": [], "at": "00:00"}, "rules": None},
            {"name": "Meals", "fields": [{"key": "kcal", "label": "Calories", "kind": "number", "unit": "kcal"}],
             "cadence": {"freq": "daily", "weekdays": [], "at": meals_at}, "rules": None},
        ],
    }
    makeup = (now + datetime.timedelta(days=1)).replace(hour=18, minute=0, second=0, microsecond=0)
    proposal = {"summary": "Add a make-up core session tomorrow evening for the missed workout.", "size": "big",
                "changes": [{"op": "create", "action_id": None, "title": "Make-up core workout", "start": makeup.strftime(FMT), "minutes": 30}]}
    key_test = {"id": "msg_k", "type": "message", "role": "assistant", "content": [{"type": "text", "text": "OK"}],
                "stop_reason": "max_tokens", "usage": {"input_tokens": 5, "output_tokens": 1}}
    return {"rules": [
        {"unless": ["\"stream\":true"], "reply": {"json": key_test}},
        {"when": ["You are Intake interviewer"], "reply": {"text": json.dumps(questions)}},
        {"when": ["You are Goal researcher"], "reply": {"text": json.dumps(research)}},
        {"when": ["You are Goal planner"], "reply": {"text": json.dumps(plan)}},
        {"when": ["You are Adaptation analyst"], "reply": {"text": json.dumps(proposal)}},
        {"when": ["You are Log intake"], "reply": {"text": json.dumps({"entries": []})}},
        {"when": ["You are Plugin builder"], "reply": {"text": json.dumps({"bundle_toml": bundle})}},
    ]}


def calendar_seed(now):
    """Device events the calendar e2e seeds, ISO 8601 UTC: a daily standup at the workout time,
    a spin class, a dentist visit and a holiday. Input: aware local now.
    Output: the events in the triage key order e1 to e4."""
    workout, _ = plan_times(now, 60)
    day = (now + datetime.timedelta(days=1)).replace(second=0, microsecond=0)
    at = lambda t, minutes: (t.astimezone(datetime.timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ"), (t + datetime.timedelta(minutes=minutes)).astimezone(datetime.timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ"))
    rows = [("Team standup", workout, 30, "FREQ=DAILY"), ("Spin class", day.replace(hour=20, minute=0), 60, "FREQ=WEEKLY;BYDAY=" + ["MO", "TU", "WE", "TH", "FR", "SA", "SU"][day.weekday()]),
            ("Dentist", (day + datetime.timedelta(days=1)).replace(hour=15, minute=0), 45, None), ("Founders Day", (day + datetime.timedelta(days=2)).replace(hour=12, minute=0), 60, None)]
    out = []
    for title, start, minutes, rrule in rows:
        s, e = at(start, minutes)
        out.append({"title": title, "start_at": s, "end_at": e, "rrule": rrule})
    return out


def calendar_rules(now):
    """Script rules for the calendar flow, matched before the six-pack rules: the triage groups,
    and a planner reply after the busy repair that moves both items behind the standup.
    Input: aware local now. Output: the rules as a list."""
    workout, _ = plan_times(now, 60)
    groups = {"groups": [
        {"name": "Work meetings", "kind": "work", "keep": True, "reason": "Daily work meeting.", "events": ["e1"]},
        {"name": "Gym classes", "kind": "fitness", "keep": True, "reason": "Booked class.", "events": ["e2"]},
        {"name": "Appointments", "kind": "personal", "keep": True, "reason": "One-off appointment.", "events": ["e3"]},
        {"name": "Holidays", "kind": "noise", "keep": False, "reason": "No time taken.", "events": ["e4"]},
    ], "days": []}
    moved = dict(script(now, 30)["rules"][3]["reply"])
    plan = json.loads(moved["text"])
    plan["items"][0]["start"] = (workout + datetime.timedelta(minutes=30)).strftime(FMT)
    plan["items"][1]["start"] = (workout + datetime.timedelta(days=1, minutes=30)).strftime(FMT)
    return [
        {"when": ["You are Calendar triage"], "reply": {"text": json.dumps(groups)}},
        {"when": ["You are Goal planner", "overlaps busy"], "reply": {"text": json.dumps(plan)}},
    ]


def pod18_plan(now):
    """Plan items of the pod 18 flow, local: a one-off core workout tomorrow at 18:00, a weekly
    plank on tomorrow's weekday at 06:00 and a one-off stretch in three days at 19:00, so the
    Busy day has two actions to move and the Away days one to adapt. Input: aware local now."""
    day = (now + datetime.timedelta(days=1)).replace(second=0, microsecond=0)
    weekday = ["MO", "TU", "WE", "TH", "FR", "SA", "SU"][day.weekday()]
    return [
        {"title": "Core workout", "start": day.replace(hour=18, minute=0).strftime(FMT), "minutes": 30, "rrule": None, "notes": "Crunches, leg raises, side planks"},
        {"title": "Morning plank", "start": day.replace(hour=6, minute=0).strftime(FMT), "minutes": 15, "rrule": "FREQ=WEEKLY;BYDAY=" + weekday, "notes": None},
        {"title": "Evening stretch", "start": (day + datetime.timedelta(days=2)).replace(hour=19, minute=0).strftime(FMT), "minutes": 20, "rrule": None, "notes": None},
    ]


def pod18_seed(now):
    """Device events of the pod 18 flow, ISO 8601 UTC, in triage key order e1 to e4: a daily
    standup from today at 08:00, then all-day events from local midnight to midnight: a
    Conference tomorrow, Mom's birthday the day after and a two-day Vacation after that."""
    day = (now + datetime.timedelta(days=1)).replace(hour=0, minute=0, second=0, microsecond=0)
    utc = lambda t: t.astimezone(datetime.timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")
    whole = lambda first, days: (utc(day + datetime.timedelta(days=first)), utc(day + datetime.timedelta(days=first + days)))
    standup = (day - datetime.timedelta(days=1)).replace(hour=8)
    rows = [("Team standup", (utc(standup), utc(standup + datetime.timedelta(minutes=30))), "FREQ=DAILY", False),
            ("Conference", whole(0, 1), None, True), ("Mom's birthday", whole(1, 1), None, True), ("Vacation", whole(2, 2), None, True)]
    return [{"title": t, "start_at": s, "end_at": e, "rrule": r, "all_day": a} for t, (s, e), r, a in rows]


def pod18(data, plan, now):
    """Turns the six-pack script into the pod 18 one: the pod 18 plan, triage groups and day
    kinds for the seeded events, and an analyst reply for the Away trigger matched before the
    missed-action one. Input: script dict, plan dict, aware local now. Output: none, edits data."""
    plan["items"] = pod18_plan(now)
    data["rules"][3]["reply"]["text"] = json.dumps(plan)
    triage = {"groups": [
        {"name": "Work meetings", "kind": "work", "keep": True, "reason": "Daily work meeting.", "events": ["e1"]},
        {"name": "All-day events", "kind": "noise", "keep": False, "reason": "Day kinds are set on the calendar.", "events": ["e2", "e3", "e4"]},
    ], "days": [{"event": "e2", "kind": "busy"}, {"event": "e3", "kind": "reminder"}, {"event": "e4", "kind": "away"}]}
    trip = (now + datetime.timedelta(days=3)).replace(hour=8, minute=0, second=0, microsecond=0)
    away = {"summary": "On the trip: a short hotel-room stretch, no equipment needed.", "size": "big",
            "changes": [{"op": "create", "action_id": None, "title": "Hotel-room stretch", "start": trip.strftime(FMT), "minutes": 20}]}
    data["rules"] = [{"when": ["You are Calendar triage"], "reply": {"text": json.dumps(triage)}},
                     {"when": ["You are Adaptation analyst", "Away (Vacation)"], "reply": {"text": json.dumps(away)}}] + data["rules"]


def music_times(now):
    """Local starts of the music plan: a one-off listen-through and the daily practice series.
    Input: aware local now. Output: (one-off start, series start). The one-off is 100 minutes
    out (rounded to 5), clear of the six-pack workout, or 12:30 tomorrow when that is too late."""
    start = (now + datetime.timedelta(minutes=100)).replace(second=0, microsecond=0)
    start = max(start.replace(minute=start.minute - start.minute % 5), now.replace(hour=8, minute=0, second=0, microsecond=0))
    if start + datetime.timedelta(minutes=20) > now.replace(hour=22, minute=0, second=0, microsecond=0):
        start = (now + datetime.timedelta(days=1)).replace(hour=12, minute=30, second=0, microsecond=0)
    return start, (now + datetime.timedelta(days=1)).replace(hour=19, minute=0, second=0, microsecond=0)


def music_rules(now):
    """Script rules for the music flow, matched before the six-pack rules: Clair de Lune intake
    questions, the instrument teacher's research, the practice coach's plan and the music template.
    Input: aware local now. Output: the rules as a list."""
    with open(MUSIC, encoding="utf8") as f:
        bundle = f.read()
    listen, practice = music_times(now)
    questions = {"questions": [
        {"id": "level", "text": "How long have you played piano?", "kind": "text", "choices": []},
        {"id": "minutes", "text": "How many minutes a day can you practise?", "kind": "number", "choices": []},
    ]}
    research = {"summary": "Clair de Lune is an advanced intermediate piece; learn it in four sections, hands separately and slow first, then raise the tempo step by step.",
                "sources": [{"title": "Learning Clair de Lune", "url": "https://example.org/clair-de-lune"}]}
    plan = {
        "summary": "Daily 30 minute practice, one section at a time, with a weekly run-through until March.",
        "timezone": str(now.tzinfo),
        "items": [
            {"title": "Listen through Clair de Lune with the score", "start": listen.strftime(FMT), "minutes": 20, "rrule": None, "notes": "Mark the four sections"},
            {"title": "Piano practice", "start": practice.strftime(FMT), "minutes": 30, "rrule": "FREQ=DAILY", "notes": "Hands separately, slow"},
        ],
        "trackers": [
            {"name": "Practice", "fields": [{"key": "minutes", "label": "Practice", "kind": "number", "unit": "min"}],
             "cadence": {"freq": "daily", "weekdays": [], "at": "20:00"}, "rules": None},
            {"name": "Tempo", "fields": [{"key": "bpm", "label": "Tempo", "kind": "number", "unit": "bpm"}], "cadence": None, "rules": None},
            {"name": "Pieces", "fields": [{"key": "piece", "label": "Piece or passage", "kind": "text", "unit": None},
                                          {"key": "progress", "label": "Progress", "kind": "number", "unit": "%"}], "cadence": None, "rules": None},
        ],
    }
    return [
        {"when": ["You are Intake interviewer", MUSIC_GOAL], "reply": {"text": json.dumps(questions)}},
        {"when": ["You are Instrument teacher"], "reply": {"text": json.dumps(research)}},
        {"when": ["You are Practice coach"], "reply": {"text": json.dumps(plan)}},
        {"when": ["You are Plugin builder", MUSIC_GOAL], "reply": {"text": json.dumps({"bundle_toml": bundle})}},
    ]


PREFERENCE = "Plan the morning plank in the evening instead."


def pod20_plan(now):
    """Plan items of the pod 20 flow, local: tomorrow a one-off Dentist at 08:00 (fixed at
    approval, on the standup), a one-off core workout at 18:00 and a daily plank from 06:00,
    so the Busy day moves the workout and leaves the fixed Dentist and the daily plank stuck."""
    day = (now + datetime.timedelta(days=1)).replace(second=0, microsecond=0)
    return [
        {"title": "Dentist", "start": day.replace(hour=8, minute=0).strftime(FMT), "minutes": 60, "rrule": None, "notes": None},
        {"title": "Core workout", "start": day.replace(hour=18, minute=0).strftime(FMT), "minutes": 30, "rrule": None, "notes": None},
        {"title": "Morning plank", "start": day.replace(hour=6, minute=0).strftime(FMT), "minutes": 15, "rrule": "FREQ=DAILY", "notes": None},
    ]


def pod20_seed(now):
    """Device events of the pod 20 flow, ISO 8601 UTC, in triage key order e1, e2: a daily
    standup from today at 08:00 and an all-day Conference tomorrow, local midnight to midnight."""
    day = (now + datetime.timedelta(days=1)).replace(hour=0, minute=0, second=0, microsecond=0)
    utc = lambda t: t.astimezone(datetime.timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")
    standup = (day - datetime.timedelta(days=1)).replace(hour=8)
    return [{"title": "Team standup", "start_at": utc(standup), "end_at": utc(standup + datetime.timedelta(minutes=30)), "rrule": "FREQ=DAILY", "all_day": False},
            {"title": "Conference", "start_at": utc(day), "end_at": utc(day + datetime.timedelta(days=1)), "rrule": None, "all_day": True}]


def pod20(data, plan, now):
    """Turns the six-pack script into the pod 20 one: the pod 20 plan and a triage reply with
    one group for the timed standup and a Busy day pick for the all-day Conference.
    Input: script dict, plan dict, aware local now. Output: none, edits data."""
    plan["items"] = pod20_plan(now)
    data["rules"][3]["reply"]["text"] = json.dumps(plan)
    triage = {"groups": [{"name": "Work meetings", "kind": "work", "keep": True, "reason": "Daily work meeting.", "events": ["e1"]}],
              "days": [{"event": "e2", "kind": "busy"}]}
    data["rules"] = [{"when": ["You are Calendar triage"], "reply": {"text": json.dumps(triage)}}] + data["rules"]


def pod15(data, plan, now):
    """Turns the six-pack script into the pod 15 loop at local time `now`: the
    one-off workout moves to yesterday at 21:00, so it is already missed, and a
    preference analyst reply joins the rules. Mutates `data` and `plan` in place."""
    yesterday = (now - datetime.timedelta(days=1)).replace(hour=21, minute=0, second=0, microsecond=0)
    plan["items"][0]["start"] = yesterday.strftime(FMT)
    data["rules"][3]["reply"]["text"] = json.dumps(plan)
    data["rules"].append({"when": ["You are Preference analyst"], "reply": {"text": json.dumps({"fact": PREFERENCE})}})


def main():
    """Writes the script to argv[1] for zone argv[2], reminder argv[3] minutes ahead.
    Input: output path, IANA zone, minutes. Output: the file, and the local
    times it chose on stdout."""
    path, zone, minutes = sys.argv[1], sys.argv[2], int(sys.argv[3])
    flow = sys.argv[4] if len(sys.argv) > 4 else "drive.js"
    calendar = flow == "drive-calendar.js"
    music = flow == "drive-music.js"
    now = datetime.datetime.now(zoneinfo.ZoneInfo(zone))
    data = script(now, minutes)
    assert len(data["rules"]) == 7
    plan = json.loads(data["rules"][3]["reply"]["text"])
    if calendar:
        workout, plank = plan_times(now, 60)
        plan["items"][0]["start"], plan["items"][1]["start"] = workout.strftime(FMT), plank.strftime(FMT)
        data["rules"][3]["reply"]["text"] = json.dumps(plan)
        data["rules"] = calendar_rules(now) + data["rules"]
        with open(os.path.join(os.path.dirname(path), "seed.json"), "w", encoding="utf8") as f:
            json.dump(calendar_seed(now), f)
    if flow == "pod15.js":
        pod15(data, plan, now)
        assert len(data["rules"]) == 8
    if flow == "pod18.js":
        pod18(data, plan, now)
        assert len(data["rules"]) == 9
        with open(os.path.join(os.path.dirname(path), "seed.json"), "w", encoding="utf8") as f:
            json.dump(pod18_seed(now), f)
    if flow == "pod20.js":
        pod20(data, plan, now)
        assert len(data["rules"]) == 8
        with open(os.path.join(os.path.dirname(path), "seed.json"), "w", encoding="utf8") as f:
            json.dump(pod20_seed(now), f)
    if music:
        data["rules"] = data["rules"][:1] + music_rules(now) + data["rules"][1:]
        assert len(data["rules"]) == 11
    with open(path, "w", encoding="utf8") as f:
        json.dump(data, f)
    print(json.dumps({"now": now.strftime(FMT), "items": [i["start"] for i in plan["items"]], "meals_at": plan["trackers"][1]["cadence"]["at"]}))


if __name__ == "__main__":
    main()

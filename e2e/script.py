import datetime
import json
import os
import sys
import zoneinfo

FMT = "%Y-%m-%dT%H:%M:%S"
TEMPLATE = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "crates", "aroh-plugins", "templates", "fitness.toml")


def plan_times(now):
    """First one-off slot and the series start, both local and inside 06:00 to 22:00.
    Input: aware local now. Output: (workout start, plank series start) as datetimes.
    The workout is 40 minutes out (rounded to 5), or 07:00 tomorrow when that is too late."""
    start = (now + datetime.timedelta(minutes=40)).replace(second=0, microsecond=0)
    start = start.replace(minute=start.minute - start.minute % 5)
    day_start = now.replace(hour=6, minute=0, second=0, microsecond=0)
    start = max(start, day_start)
    if start + datetime.timedelta(minutes=30) > now.replace(hour=22, minute=0, second=0, microsecond=0):
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
             "cadence": {"freq": "daily", "weekdays": [], "at": "00:00"}},
            {"name": "Meals", "fields": [{"key": "kcal", "label": "Calories", "kind": "number", "unit": "kcal"}],
             "cadence": {"freq": "daily", "weekdays": [], "at": meals_at}},
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


def main():
    """Writes the script to argv[1] for zone argv[2], reminder argv[3] minutes ahead.
    Input: output path, IANA zone, minutes. Output: the file, and the local
    times it chose on stdout."""
    path, zone, minutes = sys.argv[1], sys.argv[2], int(sys.argv[3])
    now = datetime.datetime.now(zoneinfo.ZoneInfo(zone))
    data = script(now, minutes)
    assert len(data["rules"]) == 7
    with open(path, "w", encoding="utf8") as f:
        json.dump(data, f)
    plan = json.loads(data["rules"][3]["reply"]["text"])
    print(json.dumps({"now": now.strftime(FMT), "items": [i["start"] for i in plan["items"]], "meals_at": plan["trackers"][1]["cadence"]["at"]}))


if __name__ == "__main__":
    main()

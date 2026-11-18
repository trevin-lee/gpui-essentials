// Several, overlapping, with a count for the rest past `max`.
AvatarGroup::new("team")
    .max(3)
    .children(["Sam Rivera", "Alex Kim", "Jordan Lee", "Casey Park", "Riley Chen"].map(Avatar::new))

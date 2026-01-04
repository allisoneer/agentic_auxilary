- linear tools (one, or more, of the 3 crates?) and pr comments need to integrate with agentic_logging. Did we forget to integrate there with any other tools?
- universal tool could use a re-look at how useful the current CLI fucntionality actually is, and how much we have to re-implement with clap for the standard use
cases we have.
- universal tool could potentially use an ability to modify things at runtime. There is potential to create strong dynamic tool params and types and such that we
would need to use rmcp directly for currently.
- README.md could use a huge refresh. We'll be at the point where we can have all-inclusive instructions for setting up for any repo soon. Would be a lot better than
  just "Here is a list of tools" if we mentioned how they are used and what they are for and how to do the entire setup.
- Similar to the last one, a nice QoL would be to re-look at the brand-new thoughts setup experience. How can we make that more streamlined? We should probably
enforce/require a primary "thoughts" repo, and have an initial setup command that actually populates it with everything it needs. Currently it initializes the old v1
config and that's just silly. That's not used anywhere anymore.
- Instead of tracking KB for all the files we should track tokens with tiktoken - Mostly all of the thoughts files do this currently.

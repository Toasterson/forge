# Gate

A Gate in Forge is the main access to the sources used to publish packages to a distribution.

## Background

The Term Gate originates in Sun Microsystems. It was the place used to import a copy to build the full
Operating-system including provided software for distribution. In later spiritual incarnations of Solaris, like
illumos, a Gate was still the location of the stable source with the added change of forks being
more important due to the Philosophy of [Fork yeah!](https://www.youtube.com/watch?v=-zRN7XLCRhc).

## Use cases

The Gate serves the purpose of defining package independent instructions that should be managed centrally
but that are not valid for all distributions. Such as branch version or distribution wide transforms.


## Related resources

References
1. [](Gate.md)

How-to guides
1.  [Making a Gate]

Linked concepts
1.  [Component](Components.md)
2. [](Distributions.md)

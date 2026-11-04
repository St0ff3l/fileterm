#import <AppKit/AppKit.h>
#include <assert.h>

@interface TestDockTile : NSObject
@property NSSize size;
@property(strong) NSView *contentView;
@property NSUInteger displays;
@end

@implementation TestDockTile
- (void)display {
  self.displays += 1;
}
@end

int main(int argc, const char **argv) {
  @autoreleasepool {
    assert(argc == 2);
    NSBundle *bundle = [NSBundle bundleWithPath:@(argv[1])];
    NSError *error = nil;
    if (![bundle loadAndReturnError:&error]) {
      NSLog(@"Unable to load Dock plug-in: %@", error);
      return 1;
    }
    id<NSDockTilePlugIn> plugin = [[bundle principalClass] new];
    TestDockTile *tile = [TestDockTile new];
    tile.size = NSMakeSize(64, 64);
    for (NSUInteger i = 0; i < 3; i++) {
      [plugin setDockTile:(NSDockTile *)tile];
      assert([tile.contentView isKindOfClass:[NSImageView class]]);
      assert([(NSImageView *)tile.contentView image] != nil);
      assert(tile.displays == i + 1);
    }
    [plugin setDockTile:nil];
    [plugin setDockTile:nil];
    [plugin setDockTile:(NSDockTile *)tile];
    assert(tile.displays == 4);
    [plugin setDockTile:nil];
    NSLog(@"Dock plug-in lifecycle passed");
  }
  return 0;
}

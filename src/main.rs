use std::{
    thread::{
        sleep,
    },
    time::{
        Duration,
    },
    ptr::{
        NonNull,
    },
    rc::{
        Rc,
    },
    cell::{
        RefCell,
        Cell,
    },
    collections::{
        HashMap,
    },
};

use objc2::{
    ClassType,
    msg_send,
    msg_send_id,
    sel,
    declare_class,
    declare::{
        Ivar,
        IvarDrop,
    },
    rc::{
        autoreleasepool,
        Id,
        WeakId,
    },
    runtime::{
        NSObject,
    },
    mutability::{
        InteriorMutable,
    },
};

use icrate::{
    ns_string,
    Foundation::{
        NSString,
        NSArray,
        NSCopying,
    },
    AppKit::{
        NSEvent,
        NSApplication,
        NSApplicationActivationPolicyProhibited,
        NSEventMaskAny,
        NSMenu,
        NSMenuItem,
        NSStatusBar,
        NSStatusItem,
        NSStatusBarButton,
        NSButton,
        NSVariableStatusItemLength,
    },
};

use block2::{
    Block,
    ConcreteBlock,
    RcBlock,
};

fn main() {
    hello_world();
    retain_count();

    unsafe { status_bar() };
}

fn hello_world() {
    let string = ns_string!("world");
    assert_eq!(format!("hello {string}"), "hello world".to_string());

    let array = NSArray::from_id_slice(&[string.copy()]);
    assert_eq!(format!("{array:?}"), "[\"world\"]".to_string());
}

fn retain_count() {
    let obj: Id<NSObject> = NSObject::new();
    let retain_count: usize = unsafe { msg_send![&obj, retainCount] };
    assert_eq!(retain_count, 1usize);
    {
        let obj2 = obj.clone();
        let retain_count: usize = unsafe { msg_send![&obj, retainCount] };
        assert_eq!(retain_count, 2usize);
        let retain_count: usize = unsafe { msg_send![&obj2, retainCount] };
        assert_eq!(retain_count, 2usize);
    }
    let retain_count: usize = unsafe { msg_send![&obj, retainCount] };
    assert_eq!(retain_count, 1usize);

    let obj2 = obj.clone();
    autoreleasepool(|pool| {
        let retain_count: usize = unsafe { msg_send![&obj2, retainCount] };
        assert_eq!(retain_count, 2usize);

        let obj2_autoreleased = Id::autorelease(obj2, pool);
        let retain_count: usize = unsafe { msg_send![obj2_autoreleased, retainCount] };
        assert_eq!(retain_count, 2usize);

        let retain_count: usize = unsafe { msg_send![&obj, retainCount] };
        assert_eq!(retain_count, 2usize);
    });

    let retain_count: usize = unsafe { msg_send![&obj, retainCount] };
    assert_eq!(retain_count, 1usize);
}

declare_class!(
    struct MenuItemCallback {
        callback: IvarDrop<Box<RcBlock<(*mut NSMenuItem,), ()>>, "_callback">,
    }

    mod ivars;

    unsafe impl ClassType for MenuItemCallback {
        type Super = NSObject;
        type Mutability = InteriorMutable;
        const NAME: &'static str = "MenuItemCallback";
    }

    unsafe impl MenuItemCallback {
        #[method(initWithCallback:)]
        unsafe fn init(this: *mut Self, callback: *mut Block<(*mut NSMenuItem,), ()>) -> Option<NonNull<Self>> {
            let this: Option<&mut Self> = msg_send![super(this), init];
            let Some(this) = this else {
                return None;
            };

            Ivar::write(&mut this.callback, Box::new(RcBlock::copy(callback)));

            Some(NonNull::from(this))
        }

        #[method(call:)]
        unsafe fn call(&self, sender: *mut NSMenuItem) {
            self.callback.call((sender,));
        }
    }
);

impl MenuItemCallback {
    fn new(callback: &Block<(*mut NSMenuItem,), ()>) -> Id<Self> {
        unsafe { msg_send_id![Self::alloc(), initWithCallback: callback] }
    }
}

unsafe fn status_bar() {
    let run_mode: &NSString = ns_string!("kCFRunLoopDefaultMode");
    let title: &NSString = ns_string!("TEST");

    let app: Id<NSApplication> = NSApplication::sharedApplication();
    assert_eq!(app.activationPolicy(), NSApplicationActivationPolicyProhibited);

    let status_bar: Id<NSStatusBar> = NSStatusBar::systemStatusBar();
    let status_item: Id<NSStatusItem> = status_bar.statusItemWithLength(NSVariableStatusItemLength);
    let menu: Id<NSMenu> = NSMenu::new();

    status_item.setMenu(Some(&menu));
    let status_bar_button: Id<NSStatusBarButton> = status_item.button().unwrap();
    let status_bar_button: &NSButton = status_bar_button.as_super();
    status_bar_button.setTitle(title);

    let callback_holder = Rc::new(RefCell::new(HashMap::new()));
    let initial_item_count: Rc<Cell<isize>> = Rc::new(Cell::new(0));

    let configure_view = {
        let info_label_menu_item = add_label_menu_item(menu.clone(), "");
        let menu_weak = WeakId::from_id(&menu);
        let callback_holder_weak = Rc::downgrade(&callback_holder);
        let initial_item_count_weak = Rc::downgrade(&initial_item_count);
        Rc::new(move || {
            let Some(menu) = menu_weak.load() else { return };
            let Some(initial_item_count) = initial_item_count_weak.upgrade() else { return };
            let Some(callback_holder) = callback_holder_weak.upgrade() else { return };

            let initial_item_count = initial_item_count.get();

            let item_count = menu.numberOfItems();
            info_label_menu_item.setTitle(&NSString::from_str(&format!("ADDED ITEMS: {}", item_count - initial_item_count)));
            if initial_item_count >= item_count {
                info_label_menu_item.setSubmenu(None);
                info_label_menu_item.setEnabled(false);
            } else {
                let sub_menu = NSMenu::new();
                for i in initial_item_count..item_count {
                    let menu_weak = WeakId::from_id(&menu);
                    add_clickable_menu_item(sub_menu.clone(), &callback_holder, &format!("DELETE ITEM {}", i - initial_item_count), move |_| {
                        let Some(menu) = menu_weak.load() else { return };
                        menu.removeItemAtIndex(i);
                    });
                }
                info_label_menu_item.setSubmenu(Some(&sub_menu));
                info_label_menu_item.setEnabled(true);
            }
        })
    };

    {
        let menu_weak = WeakId::from_id(&menu);
        let configure_view_weak = Rc::downgrade(&configure_view);
        let callback_holder_weak = Rc::downgrade(&callback_holder);
        add_clickable_menu_item(menu.clone(), &callback_holder, "ADD CLICKABLE ITEM", move |_| {
            let Some(menu) = menu_weak.load() else { return };
            let Some(callback_holder) = callback_holder_weak.upgrade() else { return };
            let Some(configure_view) = configure_view_weak.upgrade() else { return };

            add_clickable_menu_item(menu.clone(), &callback_holder, "CLICKABLE ITEM", |_| {
                println!("CLICKED");
            });
            configure_view();
        });
    };

    {
        let menu_weak = WeakId::from_id(&menu);
        let configure_view_weak = Rc::downgrade(&configure_view);
        add_clickable_menu_item(menu.clone(), &callback_holder, "ADD LABEL ITEM", move |_| {
            let Some(menu) = menu_weak.load() else { return };
            let Some(configure_view)  = configure_view_weak.upgrade() else { return };

            add_label_menu_item(menu.clone(), "LABEL ITEM");
            configure_view();
        });
    }

    let separator_menu_item: Id<NSMenuItem> = NSMenuItem::separatorItem();
    menu.addItem(&separator_menu_item);


    initial_item_count.set(menu.numberOfItems());
    configure_view();


    // if needed app delegate
    // app.setDelegate(app_delegate);

    app.finishLaunching();
    loop {
        let app: Id<NSApplication> = app.clone();
        autoreleasepool(move |_| {
            for _ in 0..100 {
                let event: Option<Id<NSEvent>> = app.nextEventMatchingMask_untilDate_inMode_dequeue(NSEventMaskAny, None, run_mode, true);
                if let Some(event) = event {
                    app.sendEvent(&event);
                };

                app.updateWindows();
                sleep(Duration::from_millis(10));
            }
        });
    }
}

unsafe fn add_clickable_menu_item<F>(menu: Id<NSMenu>, callback_holder: &RefCell<HashMap<Id<NSMenuItem>, Id<MenuItemCallback>>>, title: &str, cb: F) -> Id<NSMenuItem>
where
    F: Fn(Id<NSMenuItem>) + 'static
{
    let block = ConcreteBlock::new(move |sender: *mut NSMenuItem| {
        let Some(sender): Option<Id<NSMenuItem>> = Id::retain(sender) else {
            return;
        };
        cb(sender);
    });
    let block = block.copy();
    let callback: Id<MenuItemCallback> = MenuItemCallback::new(&*block);
    let menu_item: Id<NSMenuItem> = NSMenuItem::initWithTitle_action_keyEquivalent(
        NSMenuItem::alloc(),
        &NSString::from_str(title),
        Some(sel!(call:)),
        &NSString::from_str(""),
    );
    menu_item.setTarget(Some(&callback));
    menu_item.setEnabled(true);
    menu.addItem(&menu_item);
    callback_holder.borrow_mut().insert(menu_item.clone(), callback);

    menu_item
}

unsafe fn add_label_menu_item(menu: Id<NSMenu>, title: &str) -> Id<NSMenuItem> {
    let title = NSString::from_str(title);
    let menu_item: Id<NSMenuItem> = NSMenuItem::initWithTitle_action_keyEquivalent(
        NSMenuItem::alloc(),
        &title,
        None,
        ns_string!(""),
    );
    menu_item.setEnabled(true);
    menu.addItem(&menu_item);

    menu_item
}

